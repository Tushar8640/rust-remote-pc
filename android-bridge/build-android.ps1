# Builds the Rust JNI bridge (iroh P2P) for Android and copies it into the app.
# Requirements: Rust + `cargo install cargo-ndk`, Android NDK, and
#   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
if (-not $env:ANDROID_NDK_HOME) {
    $ndk = Get-ChildItem "$sdk\ndk" -Directory | Sort-Object Name | Select-Object -Last 1
    $env:ANDROID_NDK_HOME = $ndk.FullName
}
$out = "$root\android-client\app\src\main\jniLibs"
Push-Location $root
try {
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -P 26 -o $out build --release -p remote-bridge
    if ($LASTEXITCODE -ne 0) { throw "cargo ndk failed" }
    # cargo-ndk also copies stray dependency .so files; the bridge is self-contained.
    Get-ChildItem $out -Recurse -Filter *.so | Where-Object Name -ne "libremote_bridge.so" | Remove-Item
} finally { Pop-Location }
