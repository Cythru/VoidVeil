# VoidVeil

> One environment variable. Complete data sovereignty.

```bash
export OPENAI_BASE_URL=https://localhost:9998/v1
```

Your existing code works. The AI still works. Anthropic never sees your data.

---

## What it is

A drop-in local proxy that sits between your application and any AI API (Anthropic, OpenAI, or any OpenAI-compatible endpoint).

Every request is anonymised before it leaves your machine. Every response is re-hydrated before it reaches your app. The token map lives in RAM and dies with the session. It is mathematically irreversible without the map.

**The provider sees this:**
```
[PERSON_0001] has a [AMOUNT_0001] contract with [ORG_0001].
[PERSON_0001] confirmed it on [DATE_0001].
```

**Your app sees this:**
```
John Smith has a £2M contract with Acme Ltd.
John confirmed it on Thursday.
```

---

## What it isn't

**Not a jailbreak.** The model still receives your content and applies its own judgment. VoidVeil anonymises entities — it does not remove safety measures. The model can still refuse harmful requests. It just never knows who you are.

---

## What gets anonymised

- Names, organisations, companies
- Email addresses, phone numbers, postcodes
- IP addresses, URLs, UUIDs
- API keys, secrets, tokens
- Monetary amounts, dates
- Pronouns and role references ("the CEO", "my manager") resolved to the same entity

Single-pass O(n) anonymiser. Deterministic within session — same entity always maps to same token, so the model reasons coherently about your data without ever seeing it.

---

## Setup

```bash
# Build
cargo build --release

# Run
./target/release/voidveil

# Point your app at it
export OPENAI_BASE_URL=https://localhost:9998/v1   # HTTPS (E2E encrypted)
# or
export OPENAI_BASE_URL=http://localhost:9999/v1    # HTTP (loopback only)
```

**First run** generates a self-signed TLS cert at `~/.voidveil/cert.pem`.

Install it once as a trusted CA:
```bash
# Android
# Settings → Security → Install certificate → CA certificate

# Linux
sudo cp ~/.voidveil/cert.pem /usr/local/share/ca-certificates/voidveil.crt
sudo update-ca-certificates
```

---

## Configuration

| Variable | Default | Description |
|---|---|---|
| `VOIDVEIL_UPSTREAM` | `https://api.anthropic.com` | Target AI API |
| `VOIDVEIL_PORT` | `9999` | HTTP port |
| `VOIDVEIL_TLS_PORT` | `9998` | HTTPS port |

Works with any OpenAI-compatible upstream — Anthropic, OpenAI, local models, self-hosted.

---

## Architecture

```
Your app (unchanged)
  │  OPENAI_BASE_URL=localhost:9998/v1
  ▼
VoidVeil (local, E2E TLS)
  │  anonymise: real data → tokens
  │  strip:     telemetry headers
  ▼
api.anthropic.com
  │  sees: tokens only
  ▼
VoidVeil
  │  rehydrate: tokens → real data
  ▼
Your app gets real response
```

Token map: session RAM only. Never written to disk. Never transmitted. Session ends — gone.

Telemetry stripped: `x-stainless-*`, `anthropic-client-*`, `x-forwarded-for`, user-agent spoofed.

---

## Companion: VoidNoise

Optional noise layer — floods traffic with authentic human-pattern queries (health questions, Tesco shopping, social searches) routed through SearX. Makes your usage pattern invisible to traffic analysis.

```bash
bash void_noise.bm start
```

---

## License

AGPL-3.0-or-later — Cythru/VoidVeil

Any derivative work, including network services, must be open sourced under the same license.

---

*Built by the Void Signal. For everyone.*
