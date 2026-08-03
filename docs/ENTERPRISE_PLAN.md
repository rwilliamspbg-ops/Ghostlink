# Ghostlink: Path to Enterprise Readiness

This is a go-to-market and commercial-trust companion to [ROADMAP.md](ROADMAP.md).
That document answers "how does Ghostlink win the product category." This one
answers "what makes a CISO, a platform team, or a paying customer trust
Ghostlink enough to run it in production and pay for it." The two are linked:
most of the credibility work below is cheap *because* Priority Zero (real
distributed inference) already shipped — this plan would be premature if it
hadn't.

**Starting point, stated plainly**: Ghostlink today is a technically strong,
early-stage open-source project with real distributed inference, real auth,
and an honest (sometimes brutally honest) documentation culture. It does not
yet have the commercial scaffolding — verified proof points, a monetization
vehicle, or enterprise trust signals — that turns "interesting project" into
"thing we pay for." This plan closes that gap in four tracks, sequenced by
what blocks what.

---

## Track A — Fix the credibility trap before publishing anything

**This has to happen first.** The user's own suggestion — "add clear
benchmarks/comparisons in README" — runs directly into a problem already
documented in this repo: [BENCHMARKS.md](BENCHMARKS.md) explicitly labels its
CPU/GPU throughput tables and its multi-node LAN table as **"UNVERIFIED,
placeholder... do not cite these tables."** The RTX 4090 numbers, the
`orca-mini`/`mistral`/`llama2-7b` figures, and the "2/3/4 node" LAN latency
table do not correspond to any real run in this repo.

Publishing a polished README with benchmark claims sourced from those tables
would be the single fastest way to torch enterprise credibility — a
technical evaluator who clones the repo and reads `BENCHMARKS.md` closely
will find the "do not cite" warning within minutes. **Do not summarize those
numbers into the README under any framing.** The fix is to replace them, not
launder them.

1. **Run the real harnesses and replace every placeholder table.**
   - `scripts/flow_perf_snapshot.py` and `cargo bench -p ghostlink-core
     --bench criterion` already produce real numbers (see the "Full-Spectrum
     Session Benchmark" section of BENCHMARKS.md — that one is genuine).
     **Done (2026-08-03)**: a real native-Linux profile now exists too (a
     4-core mini PC with no dedicated GPU) — see BENCHMARKS.md's second
     Full-Spectrum session. Still open: a discrete NVIDIA/AMD GPU, Apple
     Silicon, and a server-class CPU — every real data point so far is
     either integrated-GPU-or-none.
   - `scripts/remote_flow_benchmark.py` needs an actual two-machine LAN run
     to replace the fabricated "2/3/4 nodes" table. This is explicitly
     flagged in ROADMAP.md's Horizon 1 item #2 as well — it's the same
     underlying gap. **Done (2026-08-03)**: a real 5-run benchmark between
     this Windows laptop and a Linux mini PC (`Desk-Mini`) over an actual
     home LAN is now in BENCHMARKS.md's Multi-Node Performance section —
     genuine `TcpStream` transport, real measured `remote_bridge_write_ms`
     (10-16ms, tracking raw ICMP RTT closely). Still open: 3+ node counts, a
     hardware pair with real dedicated GPUs on both sides, and a real
     (non-synthetic) model-inference version of this test — the current
     harness's per-stage compute is a timing proxy, not real tensor math
     (see BENCHMARKS.md's caveat on this run for why, including a capacity-
     modeling quirk that had to be worked around to get a genuine 2-node
     split at all).
   - Until further multi-node LAN numbers exist beyond this single 2-node
     case, the README and any marketing material should keep scope claims
     matched to what's actually measured — don't extrapolate to "scales to
     N nodes" from one real 2-node run.
2. **Add a compact, skimmable benchmark table to the README itself**, not just
   a link to BENCHMARKS.md — enterprise evaluators and README skimmers won't
   click through. Format: hardware spec, model, tokens/sec, and a one-line
   "as of <date>, reproduce with `<command>`" so every number is falsifiable
   by the reader. This is a trust signal in itself — showing the reproduction
   command is more convincing to a technical buyer than the number.
3. **Tighten [COMPARISON.md](COMPARISON.md) into something a buyer skims in
   30 seconds.** The current table is good and honest (it already links from
   the README TOC) — the main gap is that it undersells Ghostlink's actual
   differentiators from ROADMAP.md (zero-config heterogeneous LAN clustering,
   real cross-node sharding via llama.cpp's RPC backend) in favor of generic
   "self-hosted control" language. Sharpen the "Ghostlink" row once the H1
   benchmark lands, and delete `docs/archive/comparison_sheet.md` duplication
   or clearly mark one as canonical — having two comparison docs (one in
   `docs/`, one in `docs/archive/`) linked from the README is confusing for a
   buyer trying to find the authoritative one.
4. **Audit every other doc for the same pattern** before anything ships
   externally — `docs/PERF_BASELINE.json`/`PERF_BASELINE_STRESS.json`, any
   throughput CSVs in the repo root (`throughput_results_*.csv`), and
   `GHOSTLINK_FIX_PLAN.md` referenced at the bottom of BENCHMARKS.md. If any
   of those are also placeholders, they need the same "unverified" labeling
   or removal before an enterprise audience sees them.

**Why this is Track A and not an afterthought**: enterprise buyers
(unlike hobbyists) route RFPs through security/technical review. A single
discovered fabricated number in due diligence doesn't just kill that number
— it makes the reviewer re-verify everything else you've claimed, including
things that were true. Fix the placeholders before doing any of the polish
work in Track C.

---

## Track B — Enterprise trust signals (the actual "enterprise" in enterprise-level)

README polish and a demo video get you discovered. They don't get a platform
team to approve a production deployment. That requires closing gaps
ROADMAP.md and SECURITY_MODEL.md already name honestly:

1. **`GET /api/security/audit-log` is a stub that always returns empty**
   (SECURITY_MODEL.md, ROADMAP.md Horizon 2 #5). This is one of the first
   things a security reviewer checks for a self-hosted system handling
   inference requests. Wiring it to actually record auth failures, admin
   actions, and config changes is cheap relative to its trust payoff —
   prioritize it ahead of most Horizon 2 items.
2. **RBAC / scoped multi-user API keys** — today's model is effectively
   single-operator (one API key or JWT per instance). Any team deployment
   needs per-user scoped keys before it's a credible "team" or "enterprise"
   story, not just a home-lab one.
3. **mTLS for fabric/node-to-node transport** (SECURITY_MODEL.md Roadmap
   Notes) — the API server already has real PQC-hybrid TLS; the gap is
   inter-node. Close it before claiming "production-grade" anywhere in
   marketing copy, since the current honest caveat ("trust-the-LAN posture")
   is a real objection an enterprise security reviewer will raise.
4. **`ggml-rpc-server` has no built-in authentication** (ROADMAP.md Risks) —
   this is an upstream llama.cpp limitation, but it needs an explicit
   allowlist or auth shim before "enable distributed inference" is pitched to
   anyone running on a network that isn't fully trusted. This is a real,
   currently-open security gap, not a documentation gap — flag it to
   whoever owns security review before it's in a sales conversation.
5. **A published, even lightweight, security posture page** — SECURITY_MODEL.md
   is good internal material; an enterprise-facing summary (what's encrypted,
   what's authenticated, what a pen tester would need to know, a
   responsible-disclosure contact) belongs at a stable URL a procurement
   team can link in their own review, distinct from the developer-facing doc.
6. **A SECURITY.md with a disclosure policy and contact** at the repo root —
   check if one exists; if not, this is table stakes for any project
   accepting external security reports, and its absence is itself a red flag
   in enterprise procurement checklists.

None of this needs to be "SOC 2 certified" — that's a later, much larger
investment only worth making once there's paying enterprise demand asking
for it specifically. The items above are the floor for "a security-literate
team would consider this," not full compliance.

---

## Track C — Marketing polish (do this after Track A, not before)

The user's suggestions here are right; sequencing matters:

1. **Screenshots**: capture the GUI (chat, the Monaco editor with
   Explain/Fix/Refactor diff preview, the cluster/node dashboard) at a
   realistic window size, light and dark mode. Put 3-4 in the README above
   the fold, not just the existing walkthrough GIF.
2. **Demo video on YouTube**: the repo already has a `demo/` folder
   (gitignored, with a full-length recording with audio per the README's own
   note) and a public walkthrough GIF. Turn the existing full recording into
   a 2-3 minute YouTube video: problem statement (heterogeneous LAN hardware
   sitting idle) → the actual differentiator (zero-config cluster forms,
   model too big for one box runs anyway) → CTA. Link it from the README's
   Demo section alongside the GIF, and from COMPARISON.md.
3. **Sharpen the positioning copy in the README's "Why Ghostlink" section**
   to lead with the wedge ROADMAP.md already identified precisely —
   "point Ghostlink at every machine on your LAN and it becomes one inference
   cluster in under 5 minutes, zero YAML" — rather than the current generic
   "lower-latency planning, more control" phrasing. The sharper claim is more
   memorable and more falsifiable, which cuts both ways: only ship it once
   Track A's benchmark backs it up.
4. **A one-line install script** (ROADMAP.md Horizon 1 #5) is as much a
   marketing asset as a UX one — "`curl | sh` in one line" is a screenshot-
   able moment in its own right and directly comparable to Ollama's install
   flow, which is the single most-cited UX benchmark in this space.

---

## Track D — Monetization: GitHub Sponsors + a paid tier

The license is currently plain **MIT** (see [LICENSE](../LICENSE)) — fully
permissive, no copyleft, no field-of-use restriction. This has a direct
consequence for a "paid tier" plan: anything shipped as MIT-licensed code in
this repo can be freely re-hosted, re-sold, or forked by a competitor,
including any "enterprise" feature you add to the open repo. That's fine for
GitHub Sponsors (pure goodwill/funding, no product gating), but it means a
paid *tier* needs a deliberate boundary, not just a README announcement.

1. **GitHub Sponsors — low-risk, do this first.**
   - Add `.github/FUNDING.yml` (currently absent) pointing at a GitHub
     Sponsors profile.
   - Tiers should map to something concrete: named/logo placement in the
     README for org sponsors, priority issue triage, a private Discord/office
     hours slot for individual sponsors. Avoid promising anything that
     implies a support SLA at this stage — that's Track D.3 below.
   - This requires no code changes and no licensing decision — ship it
     alongside the README polish.
2. **Decide the paid-tier shape before announcing it.** Three real options,
   not mutually exclusive:
   - **(a) Hosted control plane** — ROADMAP.md's own Horizon 3 idea #4: a
     thin, optional SaaS layer for fleet management/updates/telemetry across
     many self-hosted Ghostlink clusters. This is the cleanest model given
     MIT: the core stays fully open and self-hostable, the paid product is a
     service you run, not code you withhold. Lowest legal complexity, but the
     furthest out on the roadmap (Horizon 3, 9-18 months) — don't announce a
     date before Horizon 2 lands.
   - **(b) Commercial support/SLA contracts** on the existing open-source
     core — fastest to stand up (no new product needed), matches the
     README's existing "commercial support path" language and Project
     Status section. This is the realistic near-term paid tier: a support
     contract, not a feature gate.
   - **(c) Open-core with a separately-licensed enterprise module** (SSO/SAML,
     RBAC UI, audit-log export, air-gapped deployment tooling) — viable, but
     requires either a CLA + relicensing plan for new enterprise-only code
     (kept out of the MIT tree entirely, in a private repo) or a dual-license
     model. This needs a real decision, ideally with legal input, before any
     code is written for it — retrofitting a license boundary onto code
     that's already MIT and public is much harder than starting clean.
   - **Recommendation**: announce (b) now — it's compatible with today's
     code, today's license, and today's README claims — and treat (a) as the
     medium-term paid product once Horizon 2's RBAC/audit-log/mTLS work
     (Track B above) makes "enterprise support" a credible thing to sell.
     Defer (c) until there's a specific enterprise customer asking for a
     feature the open core structurally shouldn't have.
3. **Don't announce a paid tier before Track B's trust gaps close.** Selling
   "enterprise support" while `audit-log` is a stub that always returns
   empty is a credibility risk the moment a paying customer's security team
   asks for it. Track B and Track D.2(b) should land together, not D first.

---

## Sequencing

| Phase | Contents | Blocks |
| --- | --- | --- |
| **1 (now)** | Track A: replace placeholder benchmarks, fix comparison-doc duplication — **done 2026-08-03** | Everything downstream — do not skip |
| **2 (2-4 weeks)** | Track C marketing polish (screenshots, YouTube demo, sharpened positioning) + GitHub Sponsors (Track D.1) | Needs Phase 1's real numbers to be honest |
| **3 (parallel with Horizon 1/2 of ROADMAP.md)** | Track B trust signals (audit log, RBAC, mTLS, SECURITY.md) | Needed before Track D.2(b) announcement |
| **4** | Announce commercial support tier (Track D.2 option b) | Needs Phase 3 substantially done |
| **5 (Horizon 3 timeframe)** | Hosted control plane as the scalable paid product (Track D.2 option a) | Needs Horizon 2 of ROADMAP.md |

## How we'll know it's working

- Zero unverified/placeholder numbers anywhere a prospect can read them
  (README, COMPARISON.md, marketing site) — this is a binary pass/fail, not
  a metric to trend.
- ~~A real two-machine LAN benchmark exists and is cited by hash/date/command,
  replacing every "unverified placeholder" table in BENCHMARKS.md.~~ **Done
  2026-08-03** — one real 2-node run exists; expand to 3+ nodes and a
  dedicated-GPU pair next.
- `/api/security/audit-log` returns real events, not an empty stub.
- At least one paying support relationship or GitHub Sponsors tier live,
  tied to a concrete deliverable (not just "thanks for the money").

## Risks

- **Shipping benchmark or marketing claims ahead of Track A** is the single
  biggest self-inflicted risk here — the placeholder tables already exist in
  the repo with "do not cite" warnings attached; the risk is a future editing
  pass losing that context and copying them into the README anyway.
- **Announcing a paid tier before deciding the licensing model (Track D.2)**
  risks giving away the exact feature set a competitor needs to fork and
  compete on, or backing into a legal cleanup later. Decide the shape before
  the announcement, not after.
- **MIT + open-core tension**: any enterprise-only feature written directly
  into the current MIT tree is fork-able by definition. If option (c) is
  ever pursued, that code needs to start in a separate, differently-licensed
  repo from day one — not be extracted from the open repo after the fact.
