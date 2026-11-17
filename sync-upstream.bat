@echo off
cd /d "%~dp0"
echo Syncing upstream Javis603/token-monitor...
git -C upstream pull --ff-only
echo Updating dependencies if needed...
pushd upstream
call npm ci --omit=dev
call npm run ensure:tokscale
popd
echo Running one-shot collection test...
node bridge\collect-filtered.cjs
pause
