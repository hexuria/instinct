# ADR 0008: Interrupt never auto-applies, expressed as a type (`AutoApply`)

- Status: accepted; **moved with the code** to hexuria/nativechat `crates/autosteer` (ADR 0010).
  Instinct no longer contains `Advice` or `AutoApply`; the rule is NativeChat's.
- Date: 2026-10-05
- Task: T10 (`instinct-steer`)

## Context

Spec §6.1 and the NativeChat consumer (§7.1) require that an interrupt answer is never applied
without a human confirming a chip ("Stop this turn?"). A boolean on the advice value is easy to
ignore; a separate enum forces every consumer `match` to name the rule.

## Decision

- `Advice` wraps a `Decision` and an `AutoApply::{Allowed, Never}` flag.
- `Advice::from_decision` sets `Never` exactly when the chosen option is `interrupt`; every other
  answer (queue, steer, abstain) is `Allowed`.
- The pack's public `advise` returns `Advice`. `Pack::ask` still returns the bare `Decision` so
  replay journals stay in core types.
- Tests pin: every interrupt on the eval set carries `Never`; queue / abstain carry `Allowed`.

## Consequences

- A consumer that matches on `AutoApply` cannot auto-apply an interrupt without writing an explicit
  `Never => …` arm.
- Steer auto-apply remains a product preference (NativeChat's "user enabled auto-steer"), not a
  pack rule; the pack only forbids interrupt.
