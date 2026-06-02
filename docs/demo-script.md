# MaaS Dogfood Environment — Demo Script

## Setup (Before Demo)

**Noy — Terminal 1 (Claude Code):**
```bash
export ANTHROPIC_BASE_URL=https://maas.apps.ocp.nrt9w.sandbox311.opentlc.com/llm/ext-claude-sonnet
export ANTHROPIC_API_KEY=sk-oai-1QjU1Cmrrd6Jf6Dew_LBX22U0usOCPb2QyWM6de0QDXn2Vm4bh9e4ktkxzAwH
export NODE_TLS_REJECT_UNAUTHORIZED=0
unset CLAUDE_CODE_USE_VERTEX
unset ANTHROPIC_VERTEX_PROJECT_ID
claude
```

**Noy — Terminal 2 (Codex):**
```bash
export MAAS_API_KEY=sk-oai-1QjU1Cmrrd6Jf6Dew_LBX22U0usOCPb2QyWM6de0QDXn2Vm4bh9e4ktkxzAwH
export NODE_TLS_REJECT_UNAUTHORIZED=0
codex
```

**Noy — Terminal 3 (oc — for model swapping):**
```bash
cd ~/code/ai-gateway-payload-processing
```

**Noy — Browser tabs (open in advance):**
1. Dashboard: https://metering-dashboard-openshift-ingress.apps.ocp.nrt9w.sandbox311.opentlc.com/dashboard
2. Grafana: https://grafana-openshift-ingress.apps.ocp.nrt9w.sandbox311.opentlc.com/d/pg-metering/ (admin/admin)
3. OpenShift Console: ExternalModel CRD page
4. Flow Visualization: https://noyitz.github.io/ai-gateway-docs/claude-passthrough/

**Yossi — Terminal (Claude Code):**
```bash
export ANTHROPIC_BASE_URL=https://maas.apps.ocp.nrt9w.sandbox311.opentlc.com/llm/ext-claude-sonnet
export ANTHROPIC_API_KEY=sk-oai-1YcGNAb4OlrgaZ6tv_toG0q1i6iWbOiPdyaVEn9Gfo3n8Tav6zPKtBRn5Ju6r
export NODE_TLS_REJECT_UNAUTHORIZED=0
unset CLAUDE_CODE_USE_VERTEX
unset ANTHROPIC_VERTEX_PROJECT_ID
claude
```

---

## Part 1: Introduction (Noy — 2 min)

**Talking points:**

> "As part of the MaaS early adopters program, we had the opportunity to use Red Hat's own engineering as an early adopter. We set up a dogfood environment where our teams — the Octopus team and the AI Engineering team — use MaaS as their gateway to AI coding tools like Claude Code and OpenAI Codex."

> "What we're about to show is how MaaS provides centralized visibility, cost tracking, and model management for AI coding tools across the organization — without developers changing anything in their workflow."

> "Developers get a single MaaS API key that works for both Claude Code and Codex. Behind the scenes, MaaS handles authentication, credential management, usage tracking, and even lets us swap the backend model transparently."

---

## Part 2: Dashboard Walkthrough (Noy — 3 min)

**[Switch to Dashboard tab]**

> "This is our MaaS Dogfood dashboard. Let me walk you through what we see."

**KPI cards at the top:**
> "We have 30 active users across two groups — Octopus and AI Engineering — generating about [X] requests, [X] million tokens, at an estimated cost of $[X]."

**Group filter buttons:**
> "I can filter by group — click 'octo' to see just the Octopus team, or 'ai-eng' for AI Engineering. Click 'All Groups' to go back."

**Pie charts:**
> "The three pie charts show usage breakdown by group, by model, and by top users. Notice we have usage across multiple models — Claude Opus, Sonnet, Haiku, GPT-5.5, GPT-5.4, and others. Each pie has toggles to switch between Cost, Tokens, and Requests views."

**Users table:**
> "The users table is sortable — I can sort by cost, tokens, or requests. Click any user to expand and see their per-model breakdown with usage bars."

**[Click on a user to expand]**

> "Here we can see exactly which models each developer is using and how much each model costs them."

**Recent Activity:**
> "The Recent Activity section shows the last requests in real-time. When we send requests during this demo, you'll see them appear here."

**[Briefly switch to Grafana tab]**

> "We also have the same data available in Grafana for teams that prefer that interface — both the native Limitador metrics from Kuadrant and the PostgreSQL-backed token metering with cost calculations."

---

## Part 3: Architecture (Noy — 2 min)

**[Switch to Flow Visualization tab]**

> "Here's how the flow works. Both Claude Code and Codex connect to the same MaaS gateway endpoint. The developer's MaaS API key is validated by Kuadrant's AuthPolicy, the user identity is injected, and the request flows through our Inference Payload Processor — the IPP — which handles model resolution, API key injection, and usage metering."

> "The key point is: developers never see the real provider API keys. Those live in Kubernetes Secrets. MaaS handles the credential swap transparently."

> "The IPP also extracts token usage from every response and records it to PostgreSQL — that's what powers the dashboard we just saw."

---

## Part 4: Live Demo — Claude Code (Noy — 3 min)

**[Switch to Terminal 1 — Claude Code]**

> "Let me show this in action. I'm running Claude Code, connected through MaaS. Let me ask it something."

**Type in Claude Code:**
```
What is the capital of France? Answer in one sentence.
```

**[Wait for response, then switch to Dashboard]**

> "If I refresh the dashboard, you can see my request just appeared in Recent Activity — showing my username, the model, tokens used, and cost."

**[Switch model in Claude Code: /model → select Sonnet]**

> "Now let me switch to a different model. I'll use /model to switch to Sonnet — a cheaper model."

**Type in Claude Code:**
```
What is the capital of Japan? Answer in one sentence.
```

**[Switch to Dashboard, hit Refresh]**

> "You can see the new request came in with a different model — Sonnet instead of Opus — and at a lower cost per token."

---

## Part 5: Live Demo — Codex (Noy — 2 min)

**[Switch to Terminal 2 — Codex]**

> "Now let me switch to a completely different client — OpenAI Codex. Same MaaS API key, different AI provider."

**Type in Codex:**
```
list the files in the current directory
```

**[Wait for response, then switch to Dashboard, hit Refresh]**

> "There it is — the Codex request shows up in the same dashboard, same user, but now with the GPT-5.5 model and OpenAI as the provider. One unified view across both AI coding tools."

---

## Part 6: Multi-User (Yossi joins — 2 min)

> "Now let's see what happens when another developer joins. Yossi, go ahead."

**Yossi — Type in Claude Code:**
```
What is the population of Tel Aviv? Answer in one sentence.
```

**[Noy switches to Dashboard, hit Refresh]**

> "There's Yossi's request — different user, same group, same dashboard. The group filter lets me see just the Octopus team's usage, or drill into any individual user."

**Yossi — Send another request:**
```
Write a hello world function in Python
```

**[Noy refreshes Dashboard again]**

> "Two requests from Yossi now. The pie charts update in real-time, the user table reflects the new data."

---

## Part 7: Transparent Model Swapping (Noy — 3 min)

> "Now let me show the most powerful capability — transparent model swapping. As an admin, I can change the backend model without any developer knowing."

**[Switch to Terminal 3 — oc]**

> "Let me first show you the current ExternalModel configuration."

```bash
oc get externalmodel ext-claude-sonnet -n llm -o jsonpath='{.spec}' | python3 -m json.tool
```

> "Right now, ext-claude-sonnet is pointing to claude-opus-4-6 on api.anthropic.com. All Claude Code users are getting Opus responses."

> "Now watch — I'm going to swap it to the simulator, an on-prem model, with a single command."

```bash
./scripts/demo-swap-to-simulator.sh
```

> "Done. No user restarts needed. Let me prove it."

**[Switch to Terminal 1 — Claude Code]**

**Type in Claude Code:**
```
What is 2 + 2? Answer in one word.
```

> "Look at the response — it now says 'llm-katan-echo' in the response, which is our on-prem simulator. The developer didn't change anything — same Claude Code session, same API key — but the backend model was swapped transparently."

**[Switch to Dashboard, hit Refresh]**

> "The dashboard shows the request went to the simulator model."

---

## Part 8: Simulator & Cost Savings in Development (Noy — 2 min)

> "Let me take a moment to talk about this simulator. This is llm-katan — a lightweight echo server that implements both the Anthropic and OpenAI API formats. It runs on a single EC2 instance."

> "Why is this important? When you're developing and testing the gateway — the IPP plugins, the auth flow, the metering pipeline — you don't want to burn real API credits. Claude Opus costs $5 per million input tokens and $25 per million output tokens. During our development, we were running hundreds of requests per day for testing. That adds up fast."

> "With the simulator behind MaaS, we can develop, test, and debug the entire pipeline end-to-end — authentication, rate limiting, metering, model swapping — all without spending a single dollar on real API calls. The simulator returns valid response formats with usage data, so our metering pipeline processes it exactly the same way."

> "And here's the beauty — switching between the simulator and real models is a single command. Same ExternalModel CRD, just change the endpoint. In production, you point to api.anthropic.com. In dev and testing, you point to the simulator. The developers don't know the difference."

> "We use this in our CI pipeline as well — all our integration tests run against the simulator through MaaS, validating the full flow without any cloud API costs."

> "Now let me swap it back to the real Anthropic API."

**[Switch to Terminal 3]**

```bash
./scripts/demo-swap-to-anthropic.sh
```

**[Switch to Terminal 1 — Claude Code]**

**Type in Claude Code:**
```
What color is the sky? Answer in one word.
```

> "And we're back on real Claude Opus — the response is a real AI answer, not the echo simulator."

---

## Part 9: Wrap-up (Noy — 1 min)

> "To summarize what we just demonstrated:"
>
> 1. **Multi-provider support** — Claude Code and OpenAI Codex through the same MaaS gateway with a single API key per developer
> 2. **Real-time usage visibility** — per-user, per-model, per-group analytics with cost tracking
> 3. **Transparent model management** — admins can swap backend models without disrupting developers — switch from Opus to Sonnet to save 40% on costs, or switch to an on-prem model for data sovereignty
> 4. **Rate limiting** — Kuadrant enforces per-user token rate limits through MaaS subscriptions
> 5. **Centralized credential management** — real provider API keys never leave the cluster, developers only see their MaaS key

> "This is running on a standard OpenShift cluster with MaaS, Kuadrant, and the AI Gateway's Inference Payload Processor. Everything we showed today is open source and available in the MaaS upstream."

---

## Quick Reference — Commands During Demo

| When | What | Command |
|------|------|---------|
| Part 4 | Claude Code prompt | `What is the capital of France? Answer in one sentence.` |
| Part 4 | Switch model | `/model` → select Sonnet |
| Part 4 | Claude Code prompt | `What is the capital of Japan? Answer in one sentence.` |
| Part 5 | Codex prompt | `list the files in the current directory` |
| Part 6 | Yossi prompt 1 | `What is the population of Tel Aviv? Answer in one sentence.` |
| Part 6 | Yossi prompt 2 | `Write a hello world function in Python` |
| Part 7 | Show ExternalModel | `oc get externalmodel ext-claude-sonnet -n llm -o jsonpath='{.spec}' \| python3 -m json.tool` |
| Part 7 | Swap to simulator | `./scripts/demo-swap-to-simulator.sh` |
| Part 7 | Test simulator | `What is 2 + 2? Answer in one word.` |
| Part 7 | Swap back | `./scripts/demo-swap-to-anthropic.sh` |
| Part 7 | Test real | `What color is the sky? Answer in one word.` |

## Timing

| Part | Duration |
|------|----------|
| 1. Introduction | 2 min |
| 2. Dashboard walkthrough | 3 min |
| 3. Architecture | 2 min |
| 4. Claude Code live | 3 min |
| 5. Codex live | 2 min |
| 6. Multi-user (Yossi) | 2 min |
| 7. Model swapping | 3 min |
| 8. Simulator & cost savings | 2 min |
| 9. Wrap-up | 1 min |
| **Total** | **~20 min** |
