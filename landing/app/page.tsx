import Hero from "@/components/Hero";
import Reveal from "@/components/Reveal";
import { DOWNLOADS, REPO } from "@/lib/links";

const features = [
  {
    title: "Connect with one invite",
    body: "Start sharing, copy the invite, paste it on the other device. No IP, port or password to type.",
  },
  {
    title: "Peer-to-peer, encrypted",
    body: "Direct QUIC connection with end-to-end encryption. A public relay is used only when a direct path is not possible.",
  },
  {
    title: "Adaptive video and audio",
    body: "Auto, Smooth, Sharp and Data saver quality modes. PC audio plays on your phone too.",
  },
  {
    title: "Real touch controls",
    body: "Tap to click, long press to right click, two-finger scroll and pinch to zoom.",
  },
  {
    title: "Clipboard sync",
    body: "Copy on one device and paste on the other.",
  },
  {
    title: "Premium relay",
    body: "Optional relay server for faster, steadier connections on difficult networks.",
  },
];

const steps = [
  "On the PC, open the app and press Start sharing.",
  "Press Copy invite and send it to your phone or another PC.",
  "On that device, press Paste invite, then Connect.",
];

export default function Home() {
  return (
    <main>
      <header className="mx-auto flex max-w-5xl items-center justify-between px-6 py-5">
        <span className="font-medium">Remote Desktop</span>
        <a
          href={REPO}
          className="text-sm text-muted transition hover:text-foreground"
        >
          GitHub
        </a>
      </header>

      <Hero />

      <section className="mx-auto max-w-5xl px-6 pt-24 pb-16">
        <Reveal>
          <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">
            Built to stay out of your way
          </h2>
        </Reveal>
        <div className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((f, i) => (
            <Reveal key={f.title} delay={(i % 3) * 0.08} className="h-full">
              <div className="h-full rounded-xl border border-line bg-card p-5">
                <h3 className="font-medium">{f.title}</h3>
                <p className="mt-2 text-sm leading-relaxed text-muted">
                  {f.body}
                </p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      <section className="mx-auto max-w-5xl px-6 py-16">
        <Reveal>
          <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">
            Three steps
          </h2>
        </Reveal>
        <ol className="mt-10 grid gap-4 sm:grid-cols-3">
          {steps.map((s, i) => (
            <li key={s} className="list-none">
              <Reveal delay={i * 0.1} className="h-full">
                <div className="h-full rounded-xl border border-line bg-card p-5">
                  <span className="font-mono text-sm text-accent">0{i + 1}</span>
                  <p className="mt-3 text-sm leading-relaxed">{s}</p>
                </div>
              </Reveal>
            </li>
          ))}
        </ol>
      </section>

      <section id="download" className="mx-auto max-w-5xl px-6 py-16">
        <Reveal>
          <div className="rounded-2xl border border-line bg-card p-8 text-center sm:p-12">
            <h2 className="text-2xl font-semibold tracking-tight sm:text-3xl">
              Get the app
            </h2>
            <p className="mx-auto mt-3 max-w-md text-muted">
              Install it on the PC you want to control and on the device you
              control it from.
            </p>
            <div className="mt-8 flex flex-wrap justify-center gap-3">
              <a
                href={DOWNLOADS.windows}
                className="rounded-lg bg-accent px-5 py-3 text-sm font-medium text-white transition hover:brightness-110"
              >
                Windows (.exe)
              </a>
              <a
                href={DOWNLOADS.android}
                className="rounded-lg border border-line px-5 py-3 text-sm font-medium transition hover:border-accent"
              >
                Android (.apk)
              </a>
            </div>
            <p className="mt-5 text-xs text-muted">
              Windows may show a SmartScreen warning for unsigned apps. On
              Android, allow installs from your browser to sideload the APK.
            </p>
          </div>
        </Reveal>
      </section>

      <footer className="mx-auto max-w-5xl px-6 py-10 text-sm text-muted">
        <a href={`${REPO}/releases`} className="hover:text-foreground">
          All releases
        </a>
        <span className="mx-2">·</span>
        <a href={REPO} className="hover:text-foreground">
          Source on GitHub
        </a>
      </footer>
    </main>
  );
}
