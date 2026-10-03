# PoolSync agent instructions

Read `docs/AI-AGENT-INSTALLATION.md` before building, installing or configuring
nodes, and `docs/CONCEPT-AND-SCENARIOS.md` for the human–AI setup model and RDP
clipboard behavior. Read the latest linked qualification report before relying
on deployment or acceptance status.

- Converse with the user in French; write code, documentation and commits in English.
- Work with the human's existing authorization; ask only for indispensable missing
  information or physical actions. Do not invent additional approval stages.
- Preserve existing identities, keys, settings, screen positions and the deliberate
  absence of an extra user password. Keep secrets outside Git and public reports.
- Use Rust agents and direct encrypted peers; retain the user's existing VPN.
  Do not introduce a permanent PoolSync hub, Flutter rewrite or mobile app implicitly.
- Distinguish developed, isolated-container-tested, deployed and physically accepted.
  Keep the reported Asus/Acer physical edge failure open until actual acceptance.
- Qualify runtime changes in the dedicated gbs-test lab before authorized promotion.
  For existing hubless agents, use the documented paused-cohort upgrade and backups;
  old install helpers overwrite legacy templates and are not safe default upgrades.
- Keep GitHub Actions, CI/CD and Vercel disabled. Documentation-only changes require
  link/schema checks and visual SVG inspection, not another production deployment.
