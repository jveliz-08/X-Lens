# Setup and operations

## Host

Linux, Rust stable, Python 3 for the simulator/optional adapter. Compile with
`cargo build --release`; Cargo.lock contains only the local package. Follow the
README local demo, then provision separate credentials for physical devices.
Do not reuse the simulator's high sequence counter for a real camera identity.
Config grammar (values shown as descriptions, not usable secrets):

```text
viewer=<32 random bytes encoded as 64 hex characters>
device.front-door=<a different 64-character hex token>
```

Config must be mode 600 and all tokens distinct. IDs match `[a-z0-9-]{1,24}`.
At most 16 device entries. `tools/setup_demo.py` uses `secrets.token_hex(32)` and
never overwrites existing config. Runtime directory is ignored by Git. The
server refuses group/world-accessible data directories; new ones use mode 700.
Use a dedicated user, private backups and disk quotas for defense in depth.
No log file or automatic archive grows over time.

## TLS boundary

Install a maintained Caddy release from its official distribution; Caddy is an
explicit external runtime dependency for non-loopback usage. Adapt
`deploy/Caddyfile` to a domain you control and follow Caddy's certificate/network
requirements. Validate configuration with `caddy validate --config ...` before
starting. The application port stays loopback-only; run proxy and origin in the
same host network namespace. Restrict incoming traffic with a firewall or VPN.
Do not configure proxy retries for uploads. The supplied example has no access
logging and rejects oversized request bodies. Review connection limits for the
actual network threat model; no bundled rate-limit plugin is required.

`deploy/x-lens.service` is an optional systemd example. Create the `xlens` user,
install the binary at `/opt/x-lens/x-lens` and private config at
`/etc/x-lens/config.secret` owned by that user. Review paths and privileges before
installing the service. Nothing in this repository deploys or starts it for you.
Caddy configuration and systemd service were not deployed during development.

## Operator API/AI adapter

The default adapter is Meta Model API. Both commands below are local dry-runs:

```sh
python3 tools/analyze.py --device demo --mode image
python3 tools/analyze.py --mode security
```

For actual explicitly authorized calls, see [Meta setup and limits](META_AI.md).
This project made **no real external provider calls**. The event-specific API
and account/model access remain to be confirmed. No background analysis runs.

The optional generic image contract remains available:

```sh
python3 tools/analyze.py --provider generic --device demo
# Only after operator authorization and provider configuration:
export XLENS_PROVIDER_URL=https://your-trusted-provider.example/analyze
export XLENS_PROVIDER_KEY_FILE=/private/path/provider.secret
python3 tools/analyze.py --provider generic --device demo --send
```

Generic request: `{"device":"demo","jpeg_base64":"..."}`; response: JSON <=64 KiB.
This generic contract is not presented as a Meta API. Secrets are mode-600 files
on the server. Redirects are rejected; system CA/hostname verification is on.
One process lock per config directory prevents concurrent analyses. Timeout or
invalid response is unknown/rejected, not proof the provider did no billable
work. The adapter never retries. Outputs are escaped JSON, never HTML or code.

## Recovery and rotation

Stop the origin before changing config or restoring a backup. Preserve the
watermark together with frame files. Rotate a device token after suspected
exposure; a new device ID is safest after NVS reset or a malicious counter jump.
Rotate viewer tokens and restart to invalidate future requests. Existing image
copies in browsers/backups cannot be remotely revoked. Remove directories for
retired devices deliberately; the server never erases unknown directories.
