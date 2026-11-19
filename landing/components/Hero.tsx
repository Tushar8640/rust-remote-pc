"use client";

import { useEffect, useRef } from "react";
import gsap from "gsap";
import Image from "next/image";
import { DOWNLOADS } from "@/lib/links";

export default function Hero() {
  const root = useRef<HTMLElement>(null);

  useEffect(() => {
    const ctx = gsap.context(() => {
      gsap.from(".h-in", {
        y: 28,
        opacity: 0,
        duration: 0.8,
        ease: "power3.out",
        stagger: 0.12,
      });
      gsap.from(".h-win", {
        y: 60,
        opacity: 0,
        duration: 1.1,
        ease: "power3.out",
        delay: 0.35,
      });
      gsap.from(".h-phone", {
        y: 80,
        opacity: 0,
        duration: 1.1,
        ease: "power3.out",
        delay: 0.55,
      });
      gsap.to(".h-phone", {
        y: -12,
        duration: 3,
        ease: "sine.inOut",
        yoyo: true,
        repeat: -1,
        delay: 1.8,
      });
      gsap.to(".h-glow", {
        scale: 1.15,
        opacity: 0.7,
        duration: 4,
        ease: "sine.inOut",
        yoyo: true,
        repeat: -1,
      });
    }, root);
    return () => ctx.revert();
  }, []);

  return (
    <section ref={root} className="relative overflow-hidden px-6 pt-16 pb-24">
      <div
        className="h-glow pointer-events-none absolute left-1/2 top-0 -z-10 h-[520px] w-[820px] -translate-x-1/2 rounded-full opacity-50 blur-[120px]"
        style={{ background: "radial-gradient(closest-side, #4f46e5, transparent)" }}
      />
      <div className="mx-auto max-w-3xl text-center">
        <p className="h-in mb-4 font-mono text-xs uppercase tracking-widest text-muted">
          Windows · Android
        </p>
        <h1 className="h-in text-4xl font-semibold tracking-tight sm:text-6xl">
          Your PC, in your pocket.
        </h1>
        <p className="h-in mx-auto mt-5 max-w-xl text-lg text-muted">
          See and control your Windows PC from your phone or another computer.
          Peer-to-peer and end-to-end encrypted. One invite, no port forwarding.
        </p>
        <div className="h-in mt-8 flex flex-wrap justify-center gap-3">
          <a
            href={DOWNLOADS.windows}
            className="rounded-lg bg-accent px-5 py-3 text-sm font-medium text-white transition hover:brightness-110"
          >
            Download for Windows
          </a>
          <a
            href={DOWNLOADS.android}
            className="rounded-lg border border-line bg-card px-5 py-3 text-sm font-medium transition hover:border-accent"
          >
            Download for Android
          </a>
        </div>
      </div>

      <div className="relative mx-auto mt-16 max-w-5xl">
        <div className="h-win overflow-hidden rounded-xl border border-line shadow-2xl shadow-black/60">
          <Image
            src="/shots/windows-share.png"
            alt="Windows app, Share this PC screen"
            width={1000}
            height={720}
            priority
            className="w-full"
          />
        </div>
        <div className="h-phone absolute -bottom-10 right-2 w-[28%] min-w-[110px] max-w-[220px] overflow-hidden rounded-[1.6rem] border-4 border-[#1c1c22] shadow-2xl shadow-black/70 sm:right-8">
          <Image
            src="/shots/android-connect.png"
            alt="Android app, Connect to a PC screen"
            width={1080}
            height={2400}
            priority
            className="w-full"
          />
        </div>
      </div>
    </section>
  );
}
