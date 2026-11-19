# Premium relay on Render

How premium works for your customers:

1. The **host** (the PC being controlled) turns on **Premium** in the app. The app then
   sends its connection through your relay server automatically.
2. The host copies a **join code** and sends it, with the password, to the person who
   should connect.
3. The **client** (desktop app or Android app) pastes the join code and connects. The client
   needs no licence, URL or settings.

Only hosts holding a licence you issued can use the relay. A join code carries a
short-lived pass (about a day), so a leaked code stops working on its own.

## Before you deploy: things to know

- **Render only exposes HTTP and WebSocket on one port.** The relay is built for that and
  runs behind Render's HTTPS. Render's docs describe web services as HTTP/WebSocket only
  and do not mention UDP, so assume UDP is not available.
- **Consequence: premium traffic goes through the relay, not directly between the two
  devices.** This is reliable on every network (including strict NAT and mobile data), but
  on a good home connection the free direct path can be faster than any relay. The value is
  consistency and control, not a guaranteed lower delay on every network. A server that
  allows UDP (a VPS) would let the relay help devices connect directly again; that mode is
  not built yet.
- **Choose the region before the first deploy.** Render lists Oregon, Ohio, Virginia,
  Frankfurt and Singapore and does not let you change it later. `render.yaml` uses
  `singapore`; edit it if your users are elsewhere.
- **Do not use the free plan.** Render's free web services spin down after 15 minutes
  without traffic. `render.yaml` sets no plan, so Render's default (paid) plan applies.
  Check current plan prices and bandwidth allowances on Render's pricing page yourself;
  a relayed 1080p session can use roughly 10-25 Mbit/s in each direction.

## 1. Deploy

1. Put this repository on GitHub, GitLab or Bitbucket (a private repo is fine). It contains
   no secrets.
2. In the Render dashboard choose **New > Blueprint**, pick the repository, and Render reads
   `render.yaml` from the root.
3. When asked for `RELAY_SECRET`, enter a long random value. This signs every licence and
   cannot be recovered from Render later, so **keep a copy somewhere safe**. To generate one
   in PowerShell:

   ```powershell
   [Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
   ```

4. Apply. The first build takes several minutes. When it is live, open
   `https://<your-service>.onrender.com/`. You should see the relay's landing page.
   The exact URL is shown at the top of the service page; it may have a suffix.

If the Blueprint is rejected, create the service by hand instead: **New > Web Service**,
runtime **Docker**, Dockerfile path `./relay-server/Dockerfile`, Docker context `.`,
a paid instance type, health check path `/`, and the `RELAY_SECRET` environment variable.

## 2. Who gets Premium: coupons and subscriptions

Premium is locked in the app until the host enters an **activation code**. There are two kinds,
both signed with `RELAY_SECRET`, so only you can create them and the relay enforces them:

| Code | For | Form | Ends |
|---|---|---|---|
| **Coupon** | You (the owner), free access | `RRPC-OWNER-XXXX-XXXX-XXXX-XXXX` | Never (revoke it to end it) |
| **Licence key** | Paying customers (subscription) | `rrpc-lic2.<id>.<end-date>.<key>` | On its end date |

Create them on your own PC with `premium-relay.exe`, using the same secret you gave Render:

```powershell
$env:RELAY_SECRET = "<the secret you gave Render>"

# Your own free coupon. Keep it private: anyone holding it has free Premium.
.\premium-relay.exe coupon owner

# A paid customer's 30-day subscription. Run it again for each renewal.
.\premium-relay.exe issue acme-corp --days 30
```

The id is 1-32 characters of `a-z`, `0-9` or `-`. Only give coupons to people you want to have
free access; customers should only ever receive licence keys.

**How customers pay.** There is no payment system built in. The app's **Subscribe** button opens
the page set in `RRPC_PURCHASE_URL` (set it when building the apps, or put `purchase_url=https://…`
in the host's `settings.txt`). That page is yours to provide: a checkout link or a contact form.
When a customer pays, you run `issue <id> --days N` and send them the licence key plus the relay
URL. When a subscription ends, the licence stops working and the app tells the host to renew. If
you later want payments to issue licences automatically, that needs a small payment webhook
service, which is not part of this repository.

To spare customers typing the URL, build with it set:
`$env:RRPC_PREMIUM_RELAY_URL = "https://…"; cargo build --release`. The Android app never needs it.

## 3. What the customer does

**On the PC to be controlled (host):**

1. Open **Premium relay**, paste the URL and the coupon or licence key, press **Test connection**.
   Expect "Connected to the premium relay". Without a valid code, Premium cannot be switched on
   and the app offers **Subscribe**.
2. On **Share this PC**, set **Connection** to **Premium**, then **Start sharing**.
3. Press **Copy join code** and send it, plus the password, to the person who will connect.

**On the connecting device (desktop or Android):** paste the join code where the Pairing ID
goes, enter the password, connect. Nothing else to set up.

Headless hosts: set `RRPC_PREMIUM=1`, `RRPC_PREMIUM_URL` and `RRPC_PREMIUM_LICENSE` (the coupon or licence key), then
run `rust-remote-pc host --password <pw>`. The join code is printed at start-up;
`rust-remote-pc join-code` prints a fresh one.

## Operations

| Task | How |
|---|---|
| Revoke a customer or coupon | Add its id to the `REVOKED_LICENSES` environment variable (comma-separated) in Render and let it redeploy. Their host can no longer register, and join codes built from that licence stop being accepted at the next connection. |
| Cap bandwidth per client | Set `CLIENT_MAX_KBPS` (kbit/s), for example `30000`. |
| Rotate everything | Change `RELAY_SECRET`. This invalidates every coupon and licence, so reissue them all. |
| Check a pass | `premium-relay.exe check <pass>` (needs `RELAY_SECRET`). |

Notes:

- A host's own pass lasts 30 days and is minted each time sharing starts. Join codes last
  about a day. Restart sharing if a host has been running for more than 30 days.
- Passes carry an absolute expiry, so a PC with a badly wrong clock will have its pass
  rejected. If **Test connection** fails but the URL is right, check the PC's date and time.
- The activation code is stored in plain text on the host PC (`%APPDATA%\rust-remote-pc\settings.txt`).
- The relay only forwards encrypted traffic. It cannot read the screen, input or password.
- The app also contacts the public n0 address-lookup service (the same one the standard mode
  uses). The relay URL and the host's public Pairing ID are visible there; keys and passes are not.

## What has and has not been tested

Tested: the Docker image built from this Dockerfile and run the way Render runs it
(`PORT`, `RELAY_SECRET`); licences issued inside the container match the local tool;
health check path `/` returns 200; a premium host registers; a desktop client and the
Android app (in an emulator) connect with only a join code; a forged pass, a revoked
licence and a malformed join code are refused; unit tests for the licence, pass and
join-code logic.

Not tested: a real Render deployment, connections through Render's HTTPS proxy (the local
tests used plain HTTP), real internet latency and throughput, and behaviour under many
simultaneous sessions.
