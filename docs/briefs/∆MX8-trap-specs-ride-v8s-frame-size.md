# Trap specs ride V8's frame size

## Background

Found on 2026-10-19 fixing lemmon's CI (`2a18f39`). A chain recurses on two stacks at once:
the `.wasm`'s own (a megabyte of its memory, the same on every machine) and V8's (984 KiB
by default, one frame per wasm call, a different size on each architecture). Which runs out
first depends on the CPU, so `trap.spec.ts`'s method-chain case failed on every CI run on
x64 while passing on arm64; it now runs on a small V8 stack of its own. The operator-chain
trap shape in `trap.spec.ts`, and `packages/core/src/lib/load.spec.ts`, which uses the same
chain, still rely on the `.wasm`'s stack running out first on V8's default stack, with
about 1.3× room on arm64 (2,185 terms against roughly 2,900 on V8's): frames 30% larger
would flip them to V8's `RangeError`.

## Scope and constraints

- Size each case's V8 stack for the stack it names (as `2a18f39` did with
  `--stack-size`), or derive the chain length from measured capacity, so neither depends
  on the architecture. The shell's behaviour does not care which stack runs out
  (`released()` in `build.ts` recognises a trapped core either way), so the specs should
  pin that, not the CPU.
