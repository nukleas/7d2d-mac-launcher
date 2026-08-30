# Undead Legacy dedicated server

Provisions 7 Days to Die + Undead Legacy on a Linux VPS, pinned to the same mod
build the launcher installs.

## The rule that drives everything

**7 Days to Die refuses connections between a client and server running
different mod builds.** Undead Legacy 2.7 publishes every few days — `main`
advanced during a single 40-minute download while this was written. So both
sides are pinned to explicit commits, and they must match:

| | Pinned in |
|---|---|
| Server | `server/setup-ul-server.sh` — `UL_PART1_SHA`, `UL_PART2_SHA` |
| Players | `src-tauri/src/channel.rs` — `EXPERIMENTAL_PARTS` |

Updating the mod means bumping both, re-running this script, and reissuing the
launcher to everyone. That is deliberate: UL updates routinely break saves, so a
whole-group update is the only sane way to move.

## Requirements

The workload is **single-thread bound** — clock speed matters more than core
count, and heavily oversubscribed budget VPS hosts underperform their spec sheet
badly here.

| | |
|---|---|
| CPU | 6+ high-clock cores, **x86_64 only** (no ARM — there is no ARM server build) |
| RAM | 12–16 GB for a modded server; blood moons are the spike that kills small boxes |
| Disk | 25 GB+ SSD |
| OS | Debian / Ubuntu |

Reference pick: **OVHcloud VPS-4** (8 vCores, 24 GB, 200 GB NVMe, 3 Gbps,
~$23/mo) in **Hillsboro, Oregon** for US-West players. Note that Hetzner's CCX
line roughly tripled in price in June 2026 and is no longer competitive here.

## Install

```bash
scp -r server/ user@your-vps:~/
ssh user@your-vps
sudo ~/server/setup-ul-server.sh
```

Re-running is safe — it updates the game, re-applies the pinned mod build, and
restarts the service.

## Ports

Open on **both** the host firewall and the provider's control panel:

| Port | Protocol | For |
|------|----------|-----|
| 26900 | TCP + UDP | game |
| 26901–26902 | UDP | Steam networking |
| 8080 / 8081 | TCP | optional web dashboard / telnet — leave shut unless needed |

## Operating

```bash
systemctl status 7dtd
journalctl -u 7dtd -f
systemctl restart 7dtd
```

Saves live under `/home/sdtd/.local/share/7DaysToDie/`. Back that up before any
mod update.

## When someone can't connect

Work down this list — the causes are ordered by how often they're the culprit:

1. **Mod build mismatch.** The launcher prints the installed build id on
   success; it must equal the SHAs in this script. This is the most common
   cause by a wide margin.
2. **Game version mismatch.** UL 2.7 needs 7DTD v2.6. A player on a different
   Steam branch won't connect regardless of mod build.
3. **EasyAntiCheat.** Must be off server-side (`EACEnabled=false`, set by this
   script). A modded server with EAC on is unjoinable no matter what players do.
4. **Ports.** Provider firewalls are a separate layer from the host firewall;
   both need opening.

Note that *connects then lags or drops* is a different failure from *can't
connect at all* — it points at the network path (relay/VPN hops, residential
upload bandwidth) or an undersized box, not at any of the above.
