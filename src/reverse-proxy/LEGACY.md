# Legacy server proxy

Connects DDNet 0.6 UDP clients to a native backend over QUIC. Uses the
[reverse proxy's setup](README.md); all protocol translation lives in this crate,
using libtw2. Keeps per-client state, without game simulation.

## Setup

1. Set the backend’s `sv.private_key_file` to persist its key. Use its startup
   public-key hash below.
2. Build and initialize:

   ```sh
   cargo build --release -p reverse-proxy --bin legacy-server-proxy
   target/release/legacy-server-proxy init \
     --backend-s2s 127.0.0.1:8315 --backend-hash <server-hash> \
     --listen-game-v4 0.0.0.0:8303 --listen-game-v6 '[::]:8304'
   target/release/legacy-server-proxy export
   ```

3. Add the exported hash to the backend’s `sv.trusted_proxy_hashes_file`
   (see [shared setup](README.md)). Restart the backend, then run:

   ```sh
   target/release/legacy-server-proxy run
   ```

Connect DDNet to the proxy's UDP port. Maps are converted and downloaded automatically.
Use `--config-dir legacy-proxy` before each command when running alongside the native proxy.

Supports native vanilla/DDRace snapshots and up to 64 players. No Teeworlds 0.7,
arbitrary Wasm modules, native account authentication or legacy RCON.
Master-server challenges are currently unsupported.

Translation caveats: [LIMITATIONS.md](LIMITATIONS.md).
