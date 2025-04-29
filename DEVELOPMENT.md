To compile wasm modules configure your rust installation:

```
# install wasm toolchain
rustup target add wasm32-unknown-unknown
# build some project
cargo build --target wasm32-unknown-unknown -p <proj>
```

You might want to optimize your wasm module further for performance:

```
# This allows full simd instructions
cargo +nightly build --target wasm32-unknown-unknown -p <proj> -Z build-std=panic_abort,std
```

Note also tools like `wasm-opt`.


Example development of a wasm module with `cargo-watch`:

On unix:

```
cargo watch -x "build --target wasm32-unknown-unknown -p <proj> --release" -s "mv target/wasm32-unknown-unknown/release/<name>.wasm ~/.config/<ddnet-rs>/mods/ui/wasm/wasm.wasm"
```

On Windows:

```
cargo watch -x "build --target wasm32-unknown-unknown -p <proj> --release" -s "xcopy target\wasm32-unknown-unknown\release\<name>.wasm env:AppData\DDNet\config\mods\ui\wasm\wasm.wasm /Y"
```

Ingame you can then press F1 and type `ui.path.name wasm/wasm`.

If cargo watch is slow, try:
`cargo install cargo-watch --locked --version 8.1.2`
https://github.com/watchexec/cargo-watch/issues/276

Simulate network jitter, linux only:
sudo tc qdisc add dev lo root netem delay 100ms 10ms 
sudo tc qdisc del dev lo root

ASan & TSan (the `--target` flag is important here!, `+nightly` might be required (after cargo)):
RUSTFLAGS="-Z sanitizer=address" cargo run --target x86_64-unknown-linux-gnu
TSAN_OPTIONS="ignore_noninstrumented_modules=1" RUSTFLAGS="-Z sanitizer=thread" cargo run --target x86_64-unknown-linux-gnu

Linux x11 mouse cursor while debugging:
install xdotool package
if you use the vscode workspace in misc/vscode it will do the following steps automatically

lldb has to execute this add start of debugging:

```
command source ${env:HOME}/.lldbinit
```

in `~/.lldbinit`:
```
target stop-hook add --one-liner "command script import  ~/lldbinit.py"
```

in `~/lldbinit.py` (no dot!):

```
#!/usr/bin/python
import os

print("Breakpoint hit!")
os.system("setxkbmap -option grab:break_actions")
os.system("xdotool key XF86Ungrab")
```
