# Render-reported deps

## Background

Render-reported deps — `render` reports which units and records a page read, so `lem dev`
(`∆Vhi`) re-renders only a change's dependents; answers D25's open question of whether a new
photo id re-renders through a render-time map read or a load-time `by` resolution
