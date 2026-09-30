# Reverse proxy

QUIC relay preserving client IPs and account certificates.
For DDNet 0.6 clients, use [legacy-server-proxy](LEGACY.md).

## Setup

1. Set the backend's `sv.private_key_file` to persist its key. Use its startup
   public-key hash below.
2. Build and initialize:

   ```sh
   cargo build --release -p reverse-proxy --bin reverse-proxy
   target/release/reverse-proxy init \
     --backend-s2s 127.0.0.1:8315 --backend-hash <server-hash> \
     --listen-game-v4 0.0.0.0:8310 --listen-game-v6 '[::]:8311'
   target/release/reverse-proxy export
   ```

3. Add the exported hash to the backend's trusted-proxy file and set
   `sv.trusted_proxy_hashes_file` to that file. On shared config storage, use the
   generated `proxy/trusted-proxies.txt`. Restart the backend to enable S2S.
4. Set `sv.resource_server_url` to an assets server accessible to clients.
   Set `sv.register false` to hide the backend's direct listing.
5. Run `target/release/reverse-proxy run`. Clients use the proxy address and hash.

Config lives in `proxy/` inside the game's config storage. `--config-dir <name>`
changes that subdirectory. `init` replaces config and keys; `init --keep` preserves
existing config. `import <proxy-hash>` adds a trusted hash.

S2S defaults to ports 8315/8316 (`sv.s2s_port_v4/v6`). Backend game ports are
discovered at startup; restart the proxy if they change.
For a custom master, put one HTTPS base URL per line in `server_list_urls.cfg`
in the shared config directory.
