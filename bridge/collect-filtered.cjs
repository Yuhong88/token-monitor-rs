'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const PROJECT_ROOT = path.resolve(__dirname, '..');
const UPSTREAM_ROOT = path.join(PROJECT_ROOT, 'upstream');
const DATA_DIR = path.join(PROJECT_ROOT, 'data');
const SNAPSHOT_PATH = path.join(DATA_DIR, 'latest-snapshot.json');

// Hard safety timeout so Node can never hang in the background during gaming or idle.
const HARD_TIMEOUT_MS = 45_000;
const safetyTimer = setTimeout(() => {
  console.error('[bridge] hard safety timeout exceeded');
  process.exit(1);
}, HARD_TIMEOUT_MS);
safetyTimer.unref();

function safeRequire(relPath, fallback = {}) {
  try {
    return require(path.join(UPSTREAM_ROOT, relPath));
  } catch (_) {
    return fallback;
  }
}

const { defaultDeviceId, writeJsonAtomic } = require(path.join(UPSTREAM_ROOT, 'src/shared/config'));
const { appVersion } = require(path.join(UPSTREAM_ROOT, 'src/shared/appVersion'));
const { normalizeHistoryIntervalMs } = require(path.join(UPSTREAM_ROOT, 'src/shared/collector'));
const {
  normalizeLimitsRefreshMode,
  normalizeLimitsRefreshMs
} = require(path.join(UPSTREAM_ROOT, 'src/shared/limits/collector'));
const { CLIENT_IDS = ['opencode', 'claude', 'antigravity', 'codex', 'lmstudio'] } = safeRequire('src/shared/clientCatalog', {});
const { LIMIT_PROVIDER_IDS = ['opencode', 'claude', 'antigravity', 'codex'], LIMIT_PROVIDER_LABELS = {} } = safeRequire('src/shared/limits/providers', {});
const { applyProjectRollups } = safeRequire('src/shared/usage', { applyProjectRollups: (s) => s });
const { runAgentOnce } = require(path.join(UPSTREAM_ROOT, 'src/agent/runtime'));
const sessionArchiveMod = safeRequire('src/shared/usage/sessionUsageArchive', {});
const sessionArchiveStoreMod = safeRequire('src/shared/usage/sessionUsageArchiveStore', {});
const cursorEventsMod = safeRequire('src/shared/providers/cursor/usageEvents', {});

const CANDIDATE_CLIENT_IDS = [
  'opencode',
  'claude',
  'antigravity',
  'codex',
  'lmstudio',
  'strata',
  'llama.cpp',
  'llamacpp',
  'llama-cpp'
];

const availableClients = new Set(Array.isArray(CLIENT_IDS) ? CLIENT_IDS.map((s) => String(s).toLowerCase()) : []);
const enabledUpstreamClients = CANDIDATE_CLIENT_IDS.filter((id) => availableClients.has(id));
const UPSTREAM_CLIENTS = (enabledUpstreamClients.length > 0
  ? enabledUpstreamClients
  : ['opencode', 'claude', 'antigravity', 'codex', 'lmstudio']
).join(',');

const availableLimitProviders = new Set(Array.isArray(LIMIT_PROVIDER_IDS) ? LIMIT_PROVIDER_IDS.map((s) => String(s).toLowerCase()) : []);
const enabledLimitProviders = CANDIDATE_CLIENT_IDS.filter((id) => availableLimitProviders.has(id));
const UPSTREAM_LIMIT_PROVIDERS = (enabledLimitProviders.length > 0
  ? enabledLimitProviders
  : ['opencode', 'claude', 'antigravity', 'codex']
).join(',');

const TARGET_TOOLS = [
  { id: 'opencode', label: 'OpenCode' },
  { id: 'claude', label: 'Claude Code' },
  { id: 'antigravity', label: 'Antigravity' },
  { id: 'codex', label: 'Codex (GPT)' },
  { id: 'lmstudio', label: 'LM Studio' },
  { id: 'strata', label: 'Strata' },
  { id: 'llama.cpp', label: 'llama.cpp' }
];

function num(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function localDayKey(date = new Date()) {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

function shiftDayKey(dayKey, deltaDays) {
  const ms = Date.parse(`${dayKey}T00:00:00Z`) + deltaDays * 86_400_000;
  return new Date(ms).toISOString().slice(0, 10);
}

function getUpstreamMeta() {
  let commit = 'unknown';
  try {
    commit = execFileSync('git', ['-C', UPSTREAM_ROOT, 'rev-parse', '--short', 'HEAD'], {
      encoding: 'utf8',
      windowsHide: true,
      timeout: 3000
    }).trim();
  } catch (_) {}
  return {
    version: appVersion(),
    commit
  };
}

function buildToolBreakdownFromPeriod(period, clientStatusMap = {}) {
  const src = period && typeof period === 'object' ? period : {};
  const sessions = Object.values(src.sessions || {});
  const sessionsByClient = new Map();
  for (const s of sessions) {
    if (!s || !s.client) continue;
    const list = sessionsByClient.get(s.client) || [];
    list.push(s);
    sessionsByClient.set(s.client, list);
  }

  let totalInput = 0;
  let totalReasoning = 0;

  const tools = TARGET_TOOLS.map(({ id, label }) => {
    const clientSessions = sessionsByClient.get(id) || [];
    const totalTokens = Math.max(0, Math.round(num(src.clients?.[id])));
    const costUsd = num(src.clientCosts?.[id]);
    const cacheReadTokens = Math.max(0, Math.round(num(src.clientCacheReads?.[id])));
    const cacheWriteTokens = Math.max(0, Math.round(num(src.clientCacheWrites?.[id])));
    const outputTokens = Math.max(0, Math.round(num(src.clientOutputs?.[id])));

    let inputTokens = 0;
    let reasoningTokens = 0;
    let lastUsedAt = '';

    for (const s of clientSessions) {
      inputTokens += Math.max(0, Math.round(num(s.inputTokens)));
      reasoningTokens += Math.max(0, Math.round(num(s.reasoningTokens)));
      if (s.lastUsedAt && (!lastUsedAt || s.lastUsedAt > lastUsedAt)) {
        lastUsedAt = s.lastUsedAt;
      }
    }

    if (inputTokens === 0 && totalTokens > 0) {
      inputTokens = Math.max(0, totalTokens - cacheReadTokens - cacheWriteTokens - outputTokens);
    }

    totalInput += inputTokens;
    totalReasoning += reasoningTokens;

    // Collect model breakdown
    const modelTokensMap = src.clientModels?.[id] || {};
    const modelCostsMap = src.clientModelCosts?.[id] || {};
    const models = Object.entries(modelTokensMap)
      .map(([name, tokens]) => ({
        name,
        tokens: Math.max(0, Math.round(num(tokens))),
        costUsd: num(modelCostsMap[name])
      }))
      .filter((m) => m.tokens > 0 || m.costUsd > 0)
      .sort((a, b) => b.tokens - a.tokens);

    // Fallback model aggregation from sessions if clientModels is sparse
    if (models.length === 0 && clientSessions.length > 0) {
      const aggTokens = new Map();
      const aggCosts = new Map();
      for (const s of clientSessions) {
        for (const [mName, mTok] of Object.entries(s.models || {})) {
          aggTokens.set(mName, (aggTokens.get(mName) || 0) + Math.max(0, Math.round(num(mTok))));
        }
        for (const [mName, mCost] of Object.entries(s.modelCosts || {})) {
          aggCosts.set(mName, (aggCosts.get(mName) || 0) + num(mCost));
        }
      }
      for (const [name, tokens] of aggTokens.entries()) {
        const cost = aggCosts.get(name) || 0;
        if (tokens > 0 || cost > 0) models.push({ name, tokens, costUsd: cost });
      }
      models.sort((a, b) => b.tokens - a.tokens);
    }

    // If upstream later adds native support for `llamacpp` / `llama-cpp`, fold into `llama.cpp`
    let effectiveTotalTokens = totalTokens;
    let effectiveCostUsd = costUsd;
    let effectiveInput = inputTokens;
    let effectiveOutput = outputTokens;
    let effectiveCr = cacheReadTokens;
    let effectiveCw = cacheWriteTokens;
    let effectiveReas = reasoningTokens;
    let effectiveLastUsed = lastUsedAt;
    let effectiveSessions = clientSessions.length;

    if (id === 'llama.cpp' && effectiveTotalTokens === 0) {
      for (const alias of ['llamacpp', 'llama-cpp']) {
        const aTok = Math.max(0, Math.round(num(src.clients?.[alias])));
        if (aTok > 0) {
          const aSess = sessionsByClient.get(alias) || [];
          effectiveTotalTokens += aTok;
          effectiveCostUsd += num(src.clientCosts?.[alias]);
          effectiveCr += Math.max(0, Math.round(num(src.clientCacheReads?.[alias])));
          effectiveCw += Math.max(0, Math.round(num(src.clientCacheWrites?.[alias])));
          effectiveOutput += Math.max(0, Math.round(num(src.clientOutputs?.[alias])));
          effectiveSessions += aSess.length;
          for (const s of aSess) {
            effectiveInput += Math.max(0, Math.round(num(s.inputTokens)));
            effectiveReas += Math.max(0, Math.round(num(s.reasoningTokens)));
            if (s.lastUsedAt && (!effectiveLastUsed || s.lastUsedAt > effectiveLastUsed)) effectiveLastUsed = s.lastUsedAt;
          }
          for (const [mName, mTok] of Object.entries(src.clientModels?.[alias] || {})) {
            models.push({
              name: mName,
              tokens: Math.max(0, Math.round(num(mTok))),
              costUsd: num(src.clientModelCosts?.[alias]?.[mName])
            });
          }
          models.sort((a, b) => b.tokens - a.tokens);
        }
      }
    }

    // Check standby / local attribution for tools not tracked directly by upstream (strata, llama.cpp)
    let status = clientStatusMap[id] || (effectiveTotalTokens > 0 ? 'active' : 'standby');
    if ((id === 'strata' || id === 'llama.cpp') && effectiveTotalTokens === 0) {
      // Check if any session provider/model mentions strata or llama.cpp / llamacpp
      const needle = id === 'strata' ? 'strata' : 'llama';
      let matchedTok = 0;
      let matchedCost = 0;
      let matchedIn = 0;
      let matchedOut = 0;
      let matchedCr = 0;
      let matchedCw = 0;
      let matchedReas = 0;
      let matchedSessions = 0;
      let matchedLast = '';
      const matchedModels = new Map();

      for (const s of sessions) {
        const provMatch = Object.keys(s.providers || {}).some((p) => p.toLowerCase().includes(needle));
        const modelMatch = Object.keys(s.models || {}).some((m) => m.toLowerCase().includes(needle));
        if (provMatch || (id === 'llama.cpp' && modelMatch && s.client === 'lmstudio')) {
          matchedSessions += 1;
          matchedTok += Math.max(0, Math.round(num(s.totalTokens)));
          matchedCost += num(s.costUsd);
          matchedIn += Math.max(0, Math.round(num(s.inputTokens)));
          matchedOut += Math.max(0, Math.round(num(s.outputTokens)));
          matchedCr += Math.max(0, Math.round(num(s.cacheReadTokens)));
          matchedCw += Math.max(0, Math.round(num(s.cacheWriteTokens)));
          matchedReas += Math.max(0, Math.round(num(s.reasoningTokens)));
          if (s.lastUsedAt && (!matchedLast || s.lastUsedAt > matchedLast)) matchedLast = s.lastUsedAt;
          for (const [mName, mTok] of Object.entries(s.models || {})) {
            if (!matchedModels.has(mName)) matchedModels.set(mName, { tokens: 0, costUsd: 0 });
            const entry = matchedModels.get(mName);
            entry.tokens += Math.max(0, Math.round(num(mTok)));
            entry.costUsd += num(s.modelCosts?.[mName]);
          }
        }
      }

      if (matchedTok > 0) {
        return {
          id,
          label: id === 'llama.cpp' ? 'llama.cpp (via Local/LM)' : label,
          status: 'active',
          totalTokens: matchedTok,
          inputTokens: matchedIn,
          outputTokens: matchedOut,
          cacheReadTokens: matchedCr,
          cacheWriteTokens: matchedCw,
          reasoningTokens: matchedReas,
          costUsd: matchedCost,
          sessionCount: matchedSessions,
          lastUsedAt: matchedLast,
          models: [...matchedModels.entries()]
            .map(([name, v]) => ({ name, tokens: v.tokens, costUsd: v.costUsd }))
            .sort((a, b) => b.tokens - a.tokens)
        };
      }
      status = 'standby';
    }

    return {
      id,
      label,
      status,
      totalTokens: effectiveTotalTokens,
      inputTokens: effectiveInput,
      outputTokens: effectiveOutput,
      cacheReadTokens: effectiveCr,
      cacheWriteTokens: effectiveCw,
      reasoningTokens: effectiveReas,
      costUsd: effectiveCostUsd,
      sessionCount: effectiveSessions,
      lastUsedAt: effectiveLastUsed,
      models
    };
  });

  const periodTotalTokens = Math.max(0, Math.round(num(src.totalTokens)));
  const periodCacheRead = Math.max(0, Math.round(num(src.cacheReadTokens)));
  const periodCacheWrite = Math.max(0, Math.round(num(src.cacheWriteTokens)));
  const periodOutput = Math.max(0, Math.round(num(src.outputTokens)));
  if (totalInput === 0 && periodTotalTokens > 0) {
    totalInput = Math.max(0, periodTotalTokens - periodCacheRead - periodCacheWrite - periodOutput);
  }

  return {
    totalTokens: periodTotalTokens,
    costUsd: num(src.costUsd),
    inputTokens: totalInput,
    outputTokens: periodOutput,
    cacheReadTokens: periodCacheRead,
    cacheWriteTokens: periodCacheWrite,
    reasoningTokens: totalReasoning,
    timedOutputTokens: Math.max(0, Math.round(num(src.timedOutputTokens))),
    timedDurationMs: Math.max(0, Math.round(num(src.timedDurationMs))),
    tools
  };
}

function buildWeekPeriod(history, monthPeriod, clientStatusMap = {}) {
  const todayKey = localDayKey(new Date());
  const startKey = shiftDayKey(todayKey, -6);
  const dailyRows = Array.isArray(history?.daily)
    ? history.daily.filter((d) => d && typeof d.date === 'string' && d.date >= startKey && d.date <= todayKey)
    : [];

  const syntheticPeriod = {
    totalTokens: 0,
    costUsd: 0,
    cacheReadTokens: 0,
    cacheWriteTokens: 0,
    outputTokens: 0,
    timedOutputTokens: 0,
    timedDurationMs: 0,
    clients: {},
    clientCosts: {},
    clientCacheReads: {},
    clientCacheWrites: {},
    clientOutputs: {},
    clientModels: {},
    clientModelCosts: {},
    sessions: {}
  };

  for (const d of dailyRows) {
    syntheticPeriod.totalTokens += Math.max(0, Math.round(num(d.tokens)));
    syntheticPeriod.costUsd += num(d.cost);
    syntheticPeriod.cacheReadTokens += Math.max(0, Math.round(num(d.cacheReadTokens)));
    syntheticPeriod.cacheWriteTokens += Math.max(0, Math.round(num(d.cacheWriteTokens)));
    syntheticPeriod.outputTokens += Math.max(0, Math.round(num(d.outputTokens)));

    for (const [client, cData] of Object.entries(d.perClient || {})) {
      syntheticPeriod.clients[client] = (syntheticPeriod.clients[client] || 0) + Math.max(0, Math.round(num(cData.tokens)));
      syntheticPeriod.clientCosts[client] = (syntheticPeriod.clientCosts[client] || 0) + num(cData.cost);
      syntheticPeriod.clientCacheReads[client] = (syntheticPeriod.clientCacheReads[client] || 0) + Math.max(0, Math.round(num(cData.cacheReadTokens)));
      syntheticPeriod.clientCacheWrites[client] = (syntheticPeriod.clientCacheWrites[client] || 0) + Math.max(0, Math.round(num(cData.cacheWriteTokens)));
      syntheticPeriod.clientOutputs[client] = (syntheticPeriod.clientOutputs[client] || 0) + Math.max(0, Math.round(num(cData.outputTokens)));
    }
  }

  // Enrich with 7-day sessions from monthPeriod/allTime for model & session metadata
  const cutoffMs = Date.parse(`${startKey}T00:00:00Z`);
  for (const [sKey, s] of Object.entries(monthPeriod?.sessions || {})) {
    const ts = s?.lastUsedAt ? Date.parse(s.lastUsedAt) : 0;
    if (ts >= cutoffMs) {
      syntheticPeriod.sessions[sKey] = s;
      const client = s.client;
      if (client) {
        syntheticPeriod.clientModels[client] ||= {};
        syntheticPeriod.clientModelCosts[client] ||= {};
        for (const [mName, mTok] of Object.entries(s.models || {})) {
          syntheticPeriod.clientModels[client][mName] = (syntheticPeriod.clientModels[client][mName] || 0) + Math.max(0, Math.round(num(mTok)));
        }
        for (const [mName, mCost] of Object.entries(s.modelCosts || {})) {
          syntheticPeriod.clientModelCosts[client][mName] = (syntheticPeriod.clientModelCosts[client][mName] || 0) + num(mCost);
        }
      }
    }
  }

  return buildToolBreakdownFromPeriod(syntheticPeriod, clientStatusMap);
}

function normalizeLimits(limitsObj) {
  const providers = Array.isArray(limitsObj?.providers) ? limitsObj.providers : [];
  const allowedProviders = new Set(CANDIDATE_CLIENT_IDS);
  return providers
    .filter((p) => p && allowedProviders.has(String(p.provider || '').toLowerCase()))
    .map((p) => {
      const windows = Array.isArray(p.windows)
        ? p.windows.map((w) => ({
            kind: String(w.kind || ''),
            label: String(w.label || w.kind || ''),
            usedPercent: w.usedPercent === null || w.usedPercent === undefined ? null : num(w.usedPercent),
            remainingPercent: w.remainingPercent === null || w.remainingPercent === undefined ? null : num(w.remainingPercent),
            resetsAt: w.resetsAt || null,
            resetDescription: String(w.resetDescription || ''),
            detail: String(w.detail || ''),
            metric: w.metric || null,
            used: w.used === null || w.used === undefined ? null : num(w.used),
            limit: w.limit === null || w.limit === undefined ? null : num(w.limit),
            remaining: w.remaining === null || w.remaining === undefined ? null : num(w.remaining),
            currency: w.currency || null,
            showMeter: w.showMeter !== false
          }))
        : [];
      return {
        provider: String(p.provider || ''),
        label: LIMIT_PROVIDER_LABELS[p.provider] || String(p.provider || ''),
        accountLabel: String(p.accountLabel || ''),
        accountName: String(p.accountName || ''),
        accountEmail: String(p.accountEmail || ''),
        status: String(p.status || 'unknown'),
        updatedAt: p.updatedAt || null,
        balanceUsd: p.balance?.amount ?? null,
        balanceCurrency: p.balance?.currency || null,
        windows
      };
    });
}

function extractRecentSessions(summary) {
  const sourceSessions = {
    ...(summary.allTime?.sessions || {}),
    ...(summary.month?.sessions || {}),
    ...(summary.today?.sessions || {})
  };
  const allowedClients = new Set(CANDIDATE_CLIENT_IDS);
  return Object.values(sourceSessions)
    .filter((s) => s && s.client && allowedClients.has(String(s.client).toLowerCase()))
    .sort((a, b) => String(b.lastUsedAt || '').localeCompare(String(a.lastUsedAt || '')))
    .slice(0, 15)
    .map((s) => ({
      client: s.client,
      sessionId: s.sessionId || '',
      title: String(s.title || s.projectLabel || s.sessionId || ''),
      projectLabel: String(s.projectLabel || ''),
      latestModel: String(s.latestModel || Object.keys(s.models || {})[0] || ''),
      totalTokens: Math.max(0, Math.round(num(s.totalTokens))),
      costUsd: num(s.costUsd),
      lastUsedAt: String(s.lastUsedAt || ''),
      contextTokens: Math.max(0, Math.round(num(s.contextTokens))),
      contextWindow: Math.max(0, Math.round(num(s.contextWindow)))
    }));
}

async function main() {
  const deviceId = defaultDeviceId();
  const upstream = getUpstreamMeta();
  const cursorUsageEvents = typeof cursorEventsMod.createCursorUsageEventIndex === 'function'
    ? cursorEventsMod.createCursorUsageEventIndex()
    : undefined;
  let sessionUsageArchive = null;

  const usageOptions = {
    clients: UPSTREAM_CLIENTS,
    allTimeSince: '2024-01-01',
    commandTimeoutMs: 25_000,
    allTimeTimeoutMs: 25_000,
    deviceId,
    agentVersion: upstream.version,
    agentRuntime: 'headless-bridge',
    projectsEnabled: true,
    historyEnabled: true,
    historyIntervalMs: normalizeHistoryIntervalMs(undefined),
    dailyHistoryArchiveEnabled: true,
    dailyHistoryArchiveWriteEnabled: false,
    anchorPersistenceEnabled: false,
    intervalMs: 5 * 60 * 1000,
    watchEnabled: false,
    watchDebounceMs: 1500,
    wslScanEnabled: false,
    onError: (error, reason) => console.error(`[bridge] (${reason}) ${error?.message || error}`),
    logger: () => {}
  };

  const limitsOptions = {
    limitsEnabled: true,
    limitProviders: UPSTREAM_LIMIT_PROVIDERS,
    limitsRefreshMode: normalizeLimitsRefreshMode(undefined),
    limitsRefreshMs: normalizeLimitsRefreshMs(undefined),
    claudeWebCookie: '',
    opencodeLocalLimitsEnabled: true,
    opencodeAmbientEnabled: true,
    opencodeCookie: ''
  };

  function transformUsage(summary, now = new Date()) {
    if (
      typeof sessionArchiveMod.sessionUsageArchiveDate === 'function'
      && typeof sessionArchiveMod.updateSessionUsageArchive === 'function'
      && typeof sessionArchiveMod.applySessionUsageArchive === 'function'
      && typeof sessionArchiveStoreMod.readSessionUsageArchiveSnapshot === 'function'
    ) {
      const archiveDate = sessionArchiveMod.sessionUsageArchiveDate(summary, now);
      sessionUsageArchive = sessionArchiveMod.updateSessionUsageArchive(
        sessionUsageArchive || sessionArchiveStoreMod.readSessionUsageArchiveSnapshot(),
        summary,
        archiveDate,
        { cursorUsageEvents }
      ).archive;
      const visibleSummary = sessionArchiveMod.applySessionUsageArchive(summary, sessionUsageArchive, {
        now: archiveDate,
        canonical: false
      });
      delete visibleSummary.sessionUsageArchiveFullScan;
      return applyProjectRollups(visibleSummary);
    }
    return applyProjectRollups(summary);
  }

  let latestSummary = null;
  await runAgentOnce({
    envelope: { deviceId, agentVersion: upstream.version, agentRuntime: 'headless-bridge' },
    usageOptions,
    limitsOptions,
    transformUsage,
    dryRun: true,
    deliver: async (summary) => {
      latestSummary = summary;
    },
    onError: (error, reason) => console.error(`[bridge] (${reason}) ${error?.message || error}`)
  });

  if (!latestSummary) {
    throw new Error('Upstream collector produced no summary');
  }

  const clientStatusMap = latestSummary.clientStatus || {};
  const snapshot = {
    schemaVersion: 1,
    updatedAt: new Date().toISOString(),
    upstreamVersion: upstream.version,
    upstreamCommit: upstream.commit,
    periods: {
      today: buildToolBreakdownFromPeriod(latestSummary.today, clientStatusMap),
      week: buildWeekPeriod(latestSummary.history, latestSummary.month || latestSummary.allTime, clientStatusMap),
      month: buildToolBreakdownFromPeriod(latestSummary.month, clientStatusMap),
      allTime: buildToolBreakdownFromPeriod(latestSummary.allTime, clientStatusMap)
    },
    limits: normalizeLimits(latestSummary.limits),
    recentSessions: extractRecentSessions(latestSummary)
  };

  fs.mkdirSync(DATA_DIR, { recursive: true });
  writeJsonAtomic(SNAPSHOT_PATH, snapshot);
  console.log('OK');
  process.exit(0);
}

main().catch((err) => {
  console.error('[bridge] fatal:', err?.stack || err);
  process.exit(1);
});
