# Rust Remote Desktop (Personal Edition)

একটি হালকা (Lightweight), হাই-পারফরম্যান্স এবং লো-ল্যাটেন্সি রিমোট ডেস্কটপ অ্যাপ্লিকেশন যা Tailscale (P2P Virtual LAN) নেটওয়ার্কের মাধ্যমে অফিসের Windows পিসি থেকে বাসার Windows পিসি রিমোটলি কন্ট্রোল করার জন্য ডিজাইন করা হয়েছে।

---

## 🚀 প্রধান বৈশিষ্ট্যসমূহ (Features)
- **Tailscale P2P Direct Connect:** পোর্ট ফরোয়ার্ডিং ছাড়াই সুরক্ষিত WireGuard টানেলে সরাসরি কানেকশন।
- **প্রি-শেয়ার্ড টোকেন ভ্যালিডেশন:** SHA-256 বেসড অথেনটিকেশন হ্যান্ডশেক।
- **রিয়েল-টাইম স্ক্রিন ক্যাপচার ও স্ট্রিমিং:** `xcap` এর মাধ্যমে লো-ওভারহেড স্ক্রিন গ্র্যাবিং এবং ফাস্ট ইমেজ/ভিডিও কম্প্রেশন।
- **রিমোট মাউস ও কিবোর্ড কন্ট্রোল:** `enigo` এর মাধ্যমে রেশিও-বেসড মাউস ট্র্যাকিং, ক্লিক, স্ক্রল এবং কিবোর্ড শর্টকাট সিমুলেশন।
- **লো-ল্যাটেন্সি GUI ভিউয়ার:** `egui` ও `eframe` চালিত রিয়েল-টাইম লাইভ ভিউয়ার, যাতে রয়েছে FPS ও Latency (RTT ms) মনিটর।
- **ক্লিপবোর্ড সিঙ্কিং:** রিমোট ও লোকাল মেশিনের মধ্যে কপি-পেস্ট টেক্সট আদান-প্রদান।

---

## 📂 প্রজেক্ট আর্কিটেকচার (Architecture)

```text
rust-remote-pc/
├── Cargo.toml
├── README.md
├── android-client/          # অ্যান্ড্রয়েড মোবাইল ক্লায়েন্ট (Kotlin & Jetpack Compose)
│   ├── app/
│   │   ├── build.gradle.kts
│   │   └── src/main/java/com/remote/rustpc/
│   │       ├── data/        # প্রোটোকল মডেল ও প্রিফারেন্সেস
│   │       ├── network/     # TCP সকেট ও বাইনারি কোডেক
│   │       └── ui/          # কম্পোজ স্ক্রিন ও ক্যানভাস ভিউয়ার
│   └── README.md
└── src/
    ├── main.rs              # CLI সাব-কমান্ড ও GUI লঞ্চার
    ├── common/              # শেয়ার্ড প্রোটোকল ও ক্রিপ্টো
    │   ├── mod.rs
    │   ├── protocol.rs      # Framed Packets (Auth, Input, VideoFrame, Clipboard)
    │   └── crypto.rs        # SHA-256 পাসওয়ার্ড হ্যাশিং ও টোকেন ভ্যালিডেশন
    ├── host/                # বাসার পিসির জন্য Host Agent
    │   ├── mod.rs
    │   ├── server.rs        # TCP লিসেনার, স্ট্রিম ও সেশন হ্যান্ডলার
    │   ├── capture.rs       # xcap স্ক্রিন ক্যাপচার
    │   ├── encoder.rs       # ফ্রেম এনকোডার (JPEG / H.264)
    │   ├── input.rs         # enigo মাউস ও কিবোর্ড এক্সিকিউটর
    │   └── clipboard.rs     # ক্লিপবোর্ড মনিটর
    └── client/              # অফিসের পিসির জন্য Client Viewer
        ├── mod.rs
        ├── connection.rs    # Host-এর সাথে কানেকশন ও প্যাকেট হ্যান্ডলার
        ├── decoder.rs       # ফ্রেম ডিকোডার
        └── ui.rs            # egui/eframe GUI ইন্টারফেস
```

---

## 🛠️ বিল্ড ও রান করার নিয়ম

### পূর্বশর্ত (Prerequisites):
১. **Rust ইনস্টল করুন:** [rustup.rs](https://rustup.rs) থেকে Rust টুলচেইন ইনস্টল করুন।
   ```powershell
   winget install Rustlang.Rustup
   ```
২. **Tailscale:** দুটি পিসিতেই Tailscale ইনস্টল ও লগইন করা থাকতে হবে। বাসার পিসির Tailscale আইপি নিন (যেমন: `100.101.102.103`)।

### ১. Host Agent চালানো (বাসার পিসিতে):
```powershell
# পোর্ট 5885 এ হোস্ট চালু করুন
cargo run --release -- host --bind 0.0.0.0:5885 --password "mysecret123" --fps 30
```

### ২. Client Viewer চালানো (অফিসের পিসিতে):
```powershell
# সরাসরি GUI ওপেন করে Tailscale IP দিয়ে কানেক্ট করুন:
cargo run --release -- client

# অথবা কমান্ড লাইনেই কানেকশন ডিটেইলস পাস করুন:
cargo run --release -- client --host 100.101.102.103:5885 --password "mysecret123"
```

### ৩. Android মোবাইল অ্যাপ চালানো (মোবাইলে স্ক্রিন দেখতে):
1. `android-client/app/build/outputs/apk/debug/app-debug.apk` ফোনে ইনস্টল করুন।
2. পিসির Tailscale IP এবং Password দিয়ে **"Connect to PC"** বাটনে ট্যাপ করুন।
3. মোবাইল স্ক্রিনে পিসির ডিসপ্লে লাইভ স্ট্রিম হবে এবং মাউস/কিবোর্ড দিয়ে কন্ট্রোল করতে পারবেন।

---

## 📊 পারফরম্যান্স ও সিকিউরিটি
- **ল্যাটেন্সি:** Tailscale P2P ডিরেক্ট রুটে ইনপুট রেসপন্স ৫০ ms এর নিচে থাকে।
- **রিসোর্স ব্যবহার:** ব্যাকগ্রাউন্ডে Host Agent খুবই কম মেমোরি ও CPU ব্যবহার করে।
- **সিকিউরিটি:** সম্পূর্ণ ডেটা Tailscale এর WireGuard এনক্রিপশন এবং অ্যাপ্লিকেশন লেভেলে SHA-256 টোকেন ভেরিফিকেশন দ্বারা সুরক্ষিত।

## ইন্টারনেট (P2P) কানেকশন: Pairing ID

Tailscale বা পোর্ট ফরোয়ার্ডিং ছাড়াই যেকোনো নেটওয়ার্ক থেকে কানেক্ট করা যায় (iroh, QUIC, end-to-end encrypted)।

1. হোস্ট পিসিতে অ্যাপ খুলুন। "Pairing ID" (৬৪ অক্ষর) দেখাবে। এটি প্রতি পিসির জন্য স্থির থাকে (`%APPDATA%\rust-remote-pc\host.key` থেকে তৈরি; এই ফাইল কাউকে দেবেন না)।
2. "Start sharing" চাপুন এবং পাসওয়ার্ড দেখুন।
3. ক্লায়েন্টে (Desktop বা Android) Pairing ID ও পাসওয়ার্ড দিয়ে Connect করুন।

সরাসরি সংযোগ না হলে iroh এর পাবলিক রিলে সার্ভার ব্যবহার হয় (শুধু এনক্রিপ্টেড ডেটা ফরোয়ার্ড করে; অপেক্ষাকৃত বেশি ল্যাটেন্সি)। পুরনো `IP:পোর্ট` (LAN/Tailscale) পদ্ধতিও আগের মতো কাজ করে।

Headless: `rust-remote-pc host --password <pw>` (Pairing ID টার্মিনালে ছাপা হয়; `--no-p2p` দিলে বন্ধ)। টেস্ট: `rust-remote-pc check -H <Pairing ID> -p <pw>`।

### Android এর native bridge পুনরায় বানানো

```powershell
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
.\android-bridge\build-android.ps1      # jniLibs/ এ libremote_bridge.so তৈরি করে
cd android-client; .\gradlew.bat assembleRelease
```

## ভিডিও কোয়ালিটি মোড

হোস্টের "Quality mode" এবং ভিউয়ারের সেশনের মধ্যেই (Desktop: ওপরের বারে ড্রপডাউন, Android: More → Quality) বদলানো যায়:

| মোড | কাজ |
|---|---|
| Auto | ব্যালান্সড; লিংক অনুযায়ী নিজে কোয়ালিটি/রেজোলিউশন ঠিক করে |
| Smooth | সর্বনিম্ন ডিলে ও বেশি ফ্রেম রেট (কোয়ালিটি আগে কমে) |
| Sharp | সবচেয়ে পরিষ্কার ছবি (ধীর লিংকে ফ্রেম রেট কমতে পারে) |
| Data saver | অর্ধেক রেজোলিউশন, মোবাইল ডেটার জন্য |

হেডলেস: `rust-remote-pc host --quality-mode sharp`। ক্লায়েন্ট টেস্ট: `rust-remote-pc check -H <ID> -p <pw> --quality-mode smooth` (শেষে `link: Direct/Relay ... ms` দেখায়)।

## Premium (নিজের রিলে সার্ভার, Render এ ডিপ্লয়)

**হোস্ট** "Share this PC" এ Connection type = **Premium** বেছে নিলে অ্যাপ নিজেই আপনার রিলে সার্ভার ব্যবহার করে। যে ডিভাইস কানেক্ট করবে তাকে শুধু **invite** পাঠান; সেখানে কোনো লাইসেন্স বা সেটিং লাগে না।
- সার্ভার ডিপ্লয় (Render), লাইসেন্স ইস্যু, রিভোক: [`relay-server/README.md`](relay-server/README.md) এবং রুটের `render.yaml`।
- হোস্ট সেটআপ: Desktop এ **Premium relay** পেজে URL ও licence key দিয়ে "Test connection"।
- লাইসেন্স ইস্যু: `premium-relay.exe issue <customer-id>` (`RELAY_SECRET` সহ)।
- হেডলেস: `RRPC_PREMIUM=1`, `RRPC_PREMIUM_URL`, `RRPC_PREMIUM_LICENSE` সেট করে `host`; `rust-remote-pc invite` নতুন কোড ছাপে।

### Premium কিভাবে পাওয়া যায় (coupon / subscription)

Premium চালু করতে অ্যাক্টিভেশন কোড লাগে; কোড ছাড়া "Premium" বেছে নেওয়া যায় না এবং **Subscribe** বাটন দেখায়। সার্ভারই কোড যাচাই করে, তাই অ্যাপ বদলে এটা এড়ানো যায় না।

- **Coupon** (শুধু মালিকের জন্য, ফ্রি, মেয়াদ নেই): `premium-relay.exe coupon owner`
- **Licence key** (গ্রাহকের সাবস্ক্রিপশন, মেয়াদ আছে): `premium-relay.exe issue <customer-id> --days 30`
- পেমেন্ট সিস্টেম বিল্ট-ইন নেই। Subscribe বাটন `RRPC_PURCHASE_URL` (বিল্ড টাইমে) বা `settings.txt` এর `purchase_url` এ থাকা পেজ খোলে; গ্রাহক পেমেন্ট করলে আপনি `issue` চালিয়ে কী পাঠান। বিস্তারিত: [`relay-server/README.md`](relay-server/README.md)।

## Android জেসচার

| জেসচার | কাজ |
|---|---|
| এক ট্যাপ | এক ক্লিক |
| পরপর দুই ট্যাপ (প্রায় একই জায়গায়) | ডাবল ক্লিক |
| লং প্রেস | রাইট ক্লিক |
| দুই আঙুলে ড্র্যাগ | স্ক্রল (উপর-নিচ ও ডান-বাম) |
| দুই আঙুলে পিঞ্চ | জুম |
| এক আঙুলে ড্র্যাগ | Direct touch এ বাম বাটন ধরে টানা; Trackpad এ কার্সর সরানো |

স্ক্রল ও জুম দুই মোডেই (Touch/Trackpad) কাজ করে। জেসচার টেস্ট: `cd android-client; .\gradlew.bat connectedDebugAndroidTest` (এমুলেটর বা ফোন চালু থাকতে হবে)। হোস্ট স্ক্রল টেস্ট: `rust-remote-pc check -H <ID> -p <pw> --scroll-y 120`।

## সহজ ব্যবহার (Invite)

কানেক্ট করতে ৩টা ধাপ, কোনো IP/পোর্ট/ID/পাসওয়ার্ড টাইপ করা লাগে না:

1. **যে পিসি কন্ট্রোল হবে (হোস্ট):** অ্যাপ খুলে **Start sharing** চাপুন, তারপর **Copy invite**।
2. Invite টা ফোন বা অন্য পিসিতে পাঠান (WhatsApp, ইমেইল, নোট)।
3. **যে ডিভাইস কন্ট্রোল করবে:** অ্যাপ খুলে **Paste invite** চাপুন, তারপর **Connect**। Android এ ক্লিপবোর্ডে invite থাকলে অ্যাপ নিজেই "Connect with it" দেখায়।

Invite এ পিসির ঠিকানা, পাসওয়ার্ড এবং (Premium হলে) রিলে ও তার পাস আছে, তাই ইনভাইট যার কাছে আছে সে শেয়ারিং চলা পর্যন্ত পিসি চালাতে পারে; Stop sharing করলে বা পাসওয়ার্ড বদলালে পুরনো invite অকেজো হয়। পাসওয়ার্ড একবার তৈরি হয়ে সেভ থাকে, তাই রিস্টার্টের পরও invite চলে। হেডলেস: `rust-remote-pc host` শুরুতেই invite ছাপে; `rust-remote-pc invite` নতুন করে ছাপে।

নেটওয়ার্ক ঠিকানা (IP:port) দিয়ে সরাসরি কানেকশন **More options → Direct connections** এ আছে, ডিফল্টে বন্ধ (পোর্ট খোলা হয় না)।
