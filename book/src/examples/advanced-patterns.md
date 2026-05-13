# Advanced Patterns (33–40)

Production-grade patterns for realtime systems, async workflows, auth, and distributed sync.

| # | File | Domain | Key Concepts |
|---|------|--------|--------------|
| 33 | `33-websocket-realtime.brief` | Realtime | WebSocket lifecycle, presence tracking, reconnect with exponential backoff |
| 34 | `34-background-jobs.brief` | Job Queue | Enqueue, process, retry with backoff, dead-letter queue, cron scheduling, graceful drain |
| 35 | `35-oauth-social.brief` | Auth | OAuth2 PKCE: state param, code exchange, token refresh, revoke, account linking |
| 36 | `36-pagination.brief` | Data | Cursor-based pagination, infinite scroll deduplication, pull-to-refresh, offset fallback |
| 37 | `37-webhook-handler.brief` | Integrations | HMAC signature verification, idempotency, async processing, retry budget |
| 38 | `38-kmp-shared.brief` | KMP | Kotlin Multiplatform shared module, SQLDelight, Keychain/EncryptedSharedPrefs, expect/actual |
| 39 | `39-multi-tenancy.brief` | SaaS | Tenant context resolution, RLS isolation, provisioning, quota enforcement |
| 40 | `40-conflict-resolution.brief` | Sync | Vector clock conflict detection, three-way merge, CRDT (G-Counter, ORSet), manual resolution queue |

```bash
brief check examples/33-websocket-realtime.brief
brief check examples/35-oauth-social.brief
brief check examples/38-kmp-shared.brief
brief test  examples/34-background-jobs.brief
brief test  examples/36-pagination.brief
brief test  examples/37-webhook-handler.brief
brief test  examples/40-conflict-resolution.brief
```

---

## 33 — WebSocket & Realtime

**Pattern:** WebSocket connection management with presence and exponential backoff reconnect.

Key tasks:
- `ConnectToChannel` — connect + announce presence
- `SendMessage` — ping-check then send
- `HandleIncomingMessage` — route by `MessageType` sealed variant
- `ReconnectWithBackoff` — `initialDelay=500ms`, `maxAttempts=8`, jitter
- `PresenceHeartbeat` — 30-second keep-alive
- `DisconnectGracefully` — leave presence then close

```brief
sealed type MessageType = ChatMessage(String) | TypingIndicator(String) | PresenceUpdate(String) | SystemEvent(String)

@BriefBuilder
task ReconnectWithBackoff : TaskBrief uses [WebSocket, Presence, Analytics] {
    goal   = "Reconnect a dropped WebSocket connection using exponential backoff"
    extras = ["initialDelay": "500ms", "maxDelay": "30s", "maxAttempts": "8", "jitter": "true"]

    step WaitBackoff { let delay = perform WebSocket.ping(connectionId)?; }
    step Reconnect   { let connection = perform WebSocket.connect(channelId, userId)?; }
    step RestorePresence { let presence = perform Presence.join(channelId, userId)?; }
}
```

---

## 34 — Background Jobs

**Pattern:** Job queue with retry, dead-letter, and graceful drain.

Key tasks:
- `EnqueueEmailJob` — priority queue, `ExponentialBackoff` retry policy
- `ProcessJob` — dequeue → execute → complete
- `RetryFailedJob` — compute next backoff delay, reschedule
- `DeadLetterJob` — move to DLQ and alert on-call
- `ScheduleRecurringJob` — cron `0 9 * * *` scheduling
- `DrainAndShutdown` — stop dequeuing, await in-flight

```brief
sealed type JobStatus   = Queued | Running | Succeeded | Failed(String) | Retrying(String) | DeadLettered
sealed type RetryPolicy = FixedDelay(String) | ExponentialBackoff(String) | LinearBackoff(String) | NoRetry
```

---

## 35 — OAuth2 PKCE

**Pattern:** Full OAuth2 with PKCE — prevents authorization code interception attacks.

Key tasks:
- `InitiateOAuthFlow` — generate PKCE challenge, persist state, build URL
- `HandleOAuthCallback` — validate CSRF state, exchange code, fetch profile, upsert user
- `RefreshOAuthToken` — silent refresh with stored refresh token
- `RevokeOAuthSession` — revoke at provider, clear local session
- `LinkAdditionalProvider` — account linking (reauthentication required)

Why PKCE matters: `code_challenge = BASE64URL(SHA256(code_verifier))` prevents intercepted authorization codes from being exchanged without the original verifier.

---

## 36 — Cursor Pagination

**Pattern:** GraphQL Relay-style cursor pagination with cache and deduplication.

Key tasks:
- `FetchFirstPage` — cache-check → network → cache-fill
- `FetchNextPage` — guard `hasNextPage`, deduplicate by ID, append
- `FetchPreviousPage` — guard `hasPreviousPage`, prepend
- `RefreshFirstPage` — pull-to-refresh: invalidate, re-fetch
- `SeekToPage` — offset fallback for admin/search UIs

Rule of thumb: use cursor pagination for user-facing infinite scroll; use offset pagination for admin tables with page number input.

---

## 37 — Webhook Handler

**Pattern:** Secure, idempotent webhook processing with async job dispatch.

Key tasks:
- `ReceiveWebhook` — HMAC-SHA256 verify → idempotency check → parse → route → ACK
- `ProcessPaymentWebhook` — load order, enqueue update job, record processed
- `HandleDuplicateWebhook` — look up idempotency key, acknowledge and return early
- `RetryWebhookDelivery` — exponential backoff, drop after `maxRetries=5`
- `SubscriptionLifecycleWebhook` — sync billing state, update feature access

Key principle: **verify signature before any DB writes**, and **acknowledge fast** (return 2xx immediately, do heavy work in a job queue).

---

## 38 — KMP Shared Module

**Pattern:** Kotlin Multiplatform shared business logic layer.

Key tasks:
- `InitializeSharedModule` — detect platform, open SQLDelight DB, configure analytics
- `AuthenticateShared` — load token from Keychain (iOS) / EncryptedSharedPreferences (Android)
- `SyncSharedData` — incremental delta sync, server-wins conflict strategy
- `ShareContent` — platform native share sheet via expect/actual
- `OpenExternalLink` — scheme validation then `Platform.openUrl`

The `Platform` skill maps to `expect`/`actual` declarations. iOS gets `UIActivityViewController`; Android gets `ACTION_SEND`.

---

## 39 — Multi-Tenancy

**Pattern:** SaaS tenant isolation using PostgreSQL Row-Level Security + application-level checks.

Key tasks:
- `ResolveTenantContext` — extract + cache tenant context from session (TTL 5m)
- `EnforceTenantIsolation` — permission check, deny-by-default, full audit log
- `ProvisionTenant` — create tenant, seed defaults, assign owner
- `AddTenantMember` — quota check before insert
- `SuspendTenant` — update status, invalidate all cached sessions
- `EnforceResourceQuota` — per-plan limits (users, storage, API calls)

```brief
sealed type Permission = Read | Write | Delete | Admin | Owner
sealed type TenantPlan = Free | Starter | Pro | Enterprise(String)
```

---

## 40 — Conflict Resolution

**Pattern:** Offline-first sync with vector clocks, CRDT, three-way merge, and manual fallback.

Key tasks:
- `DetectConflict` — compare vector clocks; conflict = concurrent updates (neither dominates)
- `AutoResolveConflict` — three-way merge on known base; non-overlapping changes merge cleanly
- `ApplyCRDTMerge` — CRDT join (⊔): pairwise-max G-Counter, merge ORSet tombstone sets
- `QueueManualResolution` — classify semantic conflicts, queue in-app prompt
- `ResolveManualConflict` — apply user choice (KeepMine / KeepServer / KeepBoth), bump clock
- `ReconcileAfterPartition` — batch merge all pending local changes after network heals

```brief
sealed type MergeStrategy = LastWriteWins | ServerWins | ClientWins | ThreeWayMerge | CRDT
sealed type ConflictOutcome = AutoResolved(String) | ManualResolutionRequired(String) | Discarded
```

CRDT join law: `merge(a, b) = merge(b, a)` (commutative), `merge(a, merge(b, c)) = merge(merge(a,b), c)` (associative). No coordination needed — convergence is guaranteed.
