// R2 Step 1 Shadow — TS canonical, Rust candidate-only (bounded, reversible)
// Flag: localStorage "photrez.rustShadow" === "1"  (default OFF) + autorun via PHOTREZ_SHADOW_AUTO=1
// Shadow never mutates document/history/GPU; it logs to console + window.__photrezShadowLog
// Uses existing Rust candidate `paint_parity_shadow` + `paint_parity_tiles_for_keys` (candidate-only)
// Previously working automation: PHOTREZ_PARITY_AUTO + paint_parity_export to %TEMP% (no CDP) — restored here as PHOTREZ_SHADOW_AUTO + paint_shadow_export

export function isRustShadowEnabled(): boolean {
  try { return localStorage.getItem("photrez.rustShadow") === "1"; } catch { return false; }
}

const isTauriEnv = (): boolean => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function getInvoke(): Promise<(cmd: string, args?: unknown) => Promise<unknown>> {
  const m = await import("@tauri-apps/api/core");
  return m.invoke as (cmd: string, args?: unknown) => Promise<unknown>;
}

function fnv1a(bytes: Uint8Array | Uint8ClampedArray): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) { h ^= bytes[i]; h = Math.imul(h, 0x01000193) >>> 0; }
  return ("0000000" + h.toString(16)).slice(-8);
}

function tipBytes(tip: OffscreenCanvas | HTMLCanvasElement): { w: number; h: number; data: Uint8ClampedArray } {
  const ctx = (tip as unknown as OffscreenCanvas).getContext("2d") as unknown as OffscreenCanvasRenderingContext2D;
  const im = ctx.getImageData(0, 0, tip.width, tip.height);
  return { w: tip.width, h: tip.height, data: im.data };
}

type ShadowParams = {
  w: number; h: number; brush: number; hardness: number;
  dabs: { x: number; y: number; alpha: number }[];
  tip: OffscreenCanvas | HTMLCanvasElement | null;
  eraser: boolean; prepWhite: boolean;
  tsAfter: Map<string, Uint8ClampedArray>;
  t0: number; // T0 pointerup/commit start (performance.now)
};

type ShadowRecord = {
  ts: string; w: number; h: number; brush: number; dabs: number;
  totalMs: number; t0toVisibleMs: number | null;
  rust: { prepMs: number; rasterMs: number; patchMs: number; transportMs: number; wallMs: number };
  tsDigest: string; rustDigest: string; digestEqual: boolean;
  tileCount: number; changedBytes: number;
  divergence: { max: number; histMax?: number; pooledMaxMismatch?: boolean; p50: number; p95: number; p99: number; mean: number; diffPct: number; sampledTiles: number } | null;
  perChannel: { channel: string; max: number; mean: number; diffPct: number }[] | null;
  spatial: unknown;
  wouldBeVersion: number | null;
  memRustPeakMb: number | null;
  memBaselineMb?: { current:number; peak:number } | null;
  memAfterMb?: { current:number; peak:number } | null;
  breakdown?: { tipRegMs:number; invokeWallMs:number; divergenceMs:number; memMs:number; rAFMs:number; jsOverheadMs:number; shadowEntryGapMs?:number; getInvokeMs?:number; tipReadbackMs?:number; tipArrayFromMs?:number; tipRegisterIpcMs?:number; tsDigestHashMs?:number };
  tipBlendProbe?: { abMax:number; abMean:number; bcMax:number; bcMean:number; acMax:number; acMean:number; abPerChannel?:number[]; bcPerChannel?:number[]; acPerChannel?:number[]; abChMean?:number[]; bcChMean?:number[]; acChMean?:number[]; sReadVsAMax?:number; sReadVsAMean?:number; dVsCMax?:number; dVsCMean?:number; dVsCPerChannel?:number[]; dVsBMax?:number; dVsBMean?:number; eVsAMax?:number; eVsAMean?:number } | null;
  scratchForensics?: Record<string, unknown> | null;
  parityBrush?: number | null;
  parityDump?: Record<string, unknown> | null;
  rustTiles?: { key:string; x:number; y:number; w:number; h:number; data:number[] }[] | null;
  canonicalTip?: boolean | null;
  // measurement-path separation (R2 Step 1 correction): production-like T0→T8 excludes diagnostic divergence/mem
  productionLikeT0toT8Ms?: number | null;
  diagnosticDivergenceMs?: number | null;
  error?: string;
};

// experiment-only, shadow candidate never writes canonical state
export async function runShadowForCommit(p: ShadowParams): Promise<ShadowRecord | null> {
  if (!isTauriEnv()) return null;
  const tAll0 = performance.now();
  const shadowEntryGapMs = +(performance.now() - p.t0).toFixed(3);
  const tInvGet0 = performance.now();
  const invoke = await getInvoke();
  const getInvokeMs = +(performance.now() - tInvGet0).toFixed(3);
  const tip = p.tip;
  const tTip0 = performance.now();
  const ladderForensics =
    p.w === 1024 && p.h === 1024 && p.brush === 256 &&
    Math.abs(p.hardness - 0.8) < 1e-9 && p.dabs.length <= 16;
  // ── R2 Step2 HARNESS PARITY (approved): render the DIAMETER-space stamp exactly like production TS commit.
  // Production draws rawTip(dataSize²) upscaled by drawImage to tip.diameter²; Rust must receive the
  // same upscaled bytes and the same effective stamp size, else footprints differ (274 vs 256).
  let effBrush = p.brush;
  let tb = tip ? tipBytes(tip) : { w: p.brush, h: p.brush, data: new Uint8ClampedArray(0) };
  try {
    if (tip) {
      const { getBrushTip } = await import("@/components/editor/brushTipMask");
      const D = getBrushTip({ size: p.brush, hardness: p.hardness, curve: "soft" }).diameter;
      effBrush = D;
      if (ladderForensics) {
        // C2 canonical path: Rust builds the tip from TipSpec; TS sends marker bytes only
        tb = { w: D, h: D, data: new Uint8ClampedArray(0) };
      } else if (D !== tip.width) {
        const up = new OffscreenCanvas(D, D);
        const uc = up.getContext("2d") as OffscreenCanvasRenderingContext2D;
        uc.imageSmoothingEnabled = true;
        uc.drawImage(tip, 0, 0, D, D);
        tb = { w: D, h: D, data: uc.getImageData(0, 0, D, D).data };
      }
    }
  } catch { /* parity best-effort; fall back to legacy size-space bytes */ }
  // ── R2 Step2 APPROVED dump relocation: capture EXACT post-parity bytes sent to Rust ──
  let parityDumpEarly: Record<string, unknown> | null = null;
  try {
    if (tip && tb.w) {
      const patches = [...p.tsAfter.entries()].map(([k, v]) => {
        const parts = k.split(",");
        const kx = Number(parts[0]), ky = Number(parts[1]);
        return { x: kx * 256, y: ky * 256, w: Math.min(256, p.w - kx * 256), h: Math.min(256, p.h - ky * 256), data: Array.from(v) };
      });
      parityDumpEarly = { tipW: tb.w, tipH: tb.h, tip: Array.from(tb.data), brushSent: effBrush, patches };
    }
  } catch {}
  const tipReadbackMs = +(performance.now() - tTip0).toFixed(3);
  // R2 Step2 final localization: ladder records only — relay ACTUAL Rust tile bytes
  // (paint_parity_tiles_for_keys re-runs raster_shadow deterministically => same bytes as shadow)
  let rustTilesRelay: { key:string; x:number; y:number; w:number; h:number; data:number[] }[] | null = null;
  // Build Rust request (reuse bench logic: large tips via registry)
  const base: Record<string, unknown> = {
    w: p.w, h: p.h, prep_white: p.prepWhite, eraser: p.eraser, brush: effBrush,
    dabs: p.dabs, tip_w: tb.w, tip_h: tb.h,
    hardness: p.hardness,
    canonical_tip: ladderForensics,
    tip_color: [225, 90, 23],
  };
  let tipRegMs = 0; let tipArrayFromMs = 0; let tipRegisterIpcMs = 0;
  if (tb.data.length >= 1_000_000) {
    const id = `${tb.w}x${tb.h}-${fnv1a(tb.data)}`;
    const tArr0 = performance.now();
    const tipData = Array.from(tb.data);
    tipArrayFromMs = +(performance.now() - tArr0).toFixed(3);
    const tReg0 = performance.now();
    await invoke("paint_parity_tip_register", { tipId: id, w: tb.w, h: tb.h, data: tipData }).catch(()=>{});
    tipRegisterIpcMs = +(performance.now() - tReg0).toFixed(3);
    tipRegMs = +(tipArrayFromMs + tipRegisterIpcMs).toFixed(3);
    (base as Record<string, unknown>).tip_id = id;
    (base as Record<string, unknown>).tip_data = [];
  } else {
    const tArr0 = performance.now();
    (base as Record<string, unknown>).tip_data = Array.from(tb.data);
    tipArrayFromMs = +(performance.now() - tArr0).toFixed(3);
    tipRegMs = tipArrayFromMs;
  }
  const t2wall0 = performance.now();
  let res: { digest: string; tile_count: number; changed_bytes: number; prep_us: number; raster_us: number; patchgen_us: number; tile_hashes: [string,string][] } | null = null;
  try {
    res = await invoke("paint_parity_shadow", { req: base, opts: { include_tiles: ladderForensics } }) as typeof res;
  } catch (e) {
    const rec: ShadowRecord = { ts: new Date().toISOString(), w: p.w, h: p.h, brush: p.brush, dabs: p.dabs.length, totalMs: +(performance.now()-p.t0).toFixed(3), t0toVisibleMs: null, rust: { prepMs:0,rasterMs:0,patchMs:0,transportMs:0,wallMs: +(performance.now()-t2wall0).toFixed(3) as unknown as number }, tsDigest:"", rustDigest:"", digestEqual:false, tileCount:0, changedBytes:0, divergence:null, perChannel:null, spatial:null, wouldBeVersion:null, memRustPeakMb:null, breakdown: { tipRegMs, invokeWallMs: performance.now()-t2wall0, divergenceMs:0, memMs:0, rAFMs:0, jsOverheadMs:0 }, error: String(e) };
    log(rec); return rec;
  }
  const wall = performance.now() - t2wall0;
  const computeMs = (res!.prep_us + res!.raster_us + res!.patchgen_us)/1000;
  const transportMs = Math.max(0, wall - computeMs);
  // TS digest from tsAfter map
  const tHash0 = performance.now();
  const tsHashes: [string,string][] = [...p.tsAfter.entries()].map(([k,v])=>[k, fnv1a(v)] as [string,string]).sort((a,b)=>a[0].localeCompare(b[0]));
  const tsCombined = tsHashes.map(([k,h])=>`${k}:${h};`).join("");
  const tsDigest = fnv1a(new TextEncoder().encode(tsCombined));
  const digestEqual = tsDigest === res!.digest;
  const tsDigestHashMs = +(performance.now() - tHash0).toFixed(3);
   // ── Critical path: Rust candidate → TS canonical requestRender → rAF (NO diagnostic) ──
   // Diagnostic divergence + memory IPC run AFTER T8 (below) so T0→T8 excludes them.
   const tRAf0 = performance.now();
   let t0toVisibleMs: number|null=null;
   try {
     const ed = (window as unknown as { __photrezEditor?: { scheduler?: { requestRender():void }}}).__photrezEditor;
     if (ed?.scheduler) {
       ed.scheduler.requestRender();
       await new Promise<void>(resv=> requestAnimationFrame(()=>resv()));
       t0toVisibleMs = +(performance.now()-p.t0).toFixed(3);
     }
    } catch {}
    // Stage beacon: T0→T8 complete (requestRender → rAF resolved). Diagnostics below run AFTER T8.
    try { (window as unknown as Record<string, unknown>).__photrezShadowStage = { stage: "t8", t0toVisibleMs }; } catch {}
    const rAFMs = performance.now() - tRAf0;
   const productionLikeT0toT8Ms = t0toVisibleMs; // excludes diagnostic divergence/mem below
   // ── Diagnostic (AFTER T8): divergence + tiles_for_keys + memory, timed separately ──
   let divergence: ShadowRecord["divergence"] = null;
   let perChannel: ShadowRecord["perChannel"] = null;
   let spatial: ShadowRecord["spatial"] = null;
   let divergenceMs = 0;
   if (!digestEqual) {
      // Full census (controlled probe): NO 6-key/40-key caps — compare EVERY differing tile.
      const differing = tsHashes.filter(([k,h])=> !res!.tile_hashes.find(([k2])=>k2===k) || res!.tile_hashes.find(([k2])=>k2===k)![1]!==h).map(([k])=>k);
      const budget = differing;
     if (budget.length) {
       const tDiv0 = performance.now();
       try {
         const keys = budget;
         const req2 = { ...base };
          const tiles = await invoke("paint_parity_tiles_for_keys", { req: req2, keys }) as { key:string; data: Uint8Array }[];
          if (ladderForensics) {
            try { rustTilesRelay = tiles.map((t) => ({ key: t.key, x: 0, y: 0, w: 0, h: 0, data: Array.from(t.data) })); } catch {}
          }
          // build histogram + per-channel + spatial localization (R2 Step 1 repeatability/localization)
          const hist = new Uint32Array(256);
          const chHist = [new Uint32Array(256),new Uint32Array(256),new Uint32Array(256),new Uint32Array(256)];
          const chSum=[0,0,0,0], chNz=[0,0,0,0], chMax=[0,0,0,0];
          let total=0, pixTotal=0, pixelDiffCount=0;
          let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
          // alpha buckets by source alpha value (0-63/64-127/128-191/192-255); edge buckets by alpha class
          const ab=[0,1,2,3].map(()=>({pixelCount:0,maxDiff:0,sumDiff:0}));
          const eb={ transparent:{pixelCount:0,maxDiff:0,sumDiff:0}, partial:{pixelCount:0,maxDiff:0,sumDiff:0}, opaque:{pixelCount:0,maxDiff:0,sumDiff:0} };
          for (const rt of tiles) {
            const ts = p.tsAfter.get(rt.key); if (!ts || ts.length!==rt.data.length) continue;
            const pix = ts.length/4; pixTotal+=pix;
            const side = Math.round(Math.sqrt(pix));
            const kp = rt.key.split(","); const ox = (+kp[0])*256, oy = (+kp[1])*256;
            for (let pr=0; pr<pix; pr++) {
              const px = ox + (pr % side), py = oy + Math.floor(pr / side);
              const a = ts[pr*4+3];
              let anyCh=false, pmd=0;
              for (let ch=0; ch<4; ch++) {
                const d=Math.abs(ts[pr*4+ch]-rt.data[pr*4+ch]); hist[d]++; total++; chHist[ch][d]++; chSum[ch]+=d; if(d>0){chNz[ch]++; anyCh=true;} if(d>chMax[ch]) chMax[ch]=d; if(d>pmd) pmd=d;
              }
              if (anyCh) {
                pixelDiffCount++;
                if(px<minX)minX=px; if(px>maxX)maxX=px; if(py<minY)minY=py; if(py>maxY)maxY=py;
                const bi = a>=192?3 : a>=128?2 : a>=64?1 : 0; ab[bi].pixelCount++; ab[bi].sumDiff+=pmd; if(pmd>ab[bi].maxDiff)ab[bi].maxDiff=pmd;
                const cls = a===0?eb.transparent : a===255?eb.opaque : eb.partial; cls.pixelCount++; cls.sumDiff+=pmd; if(pmd>cls.maxDiff)cls.maxDiff=pmd;
              }
            }
          }
          if (total) {
            const q=(f:number)=>{ const tgt=f*total; let acc=0; for(let d=0;d<256;d++){acc+=hist[d]; if(acc>=tgt) return d;} return 255; };
            let nz=0,sum=0,mx=0; for(let d=0;d<256;d++) if(d>0){nz+=hist[d]; sum+=d*hist[d]; mx=d;}
            // Invariant guard: pooled max MUST equal max(per-channel max) — both from the same samples.
            const chanMax = Math.max(chMax[0], chMax[1], chMax[2], chMax[3]);
            if (mx !== chanMax) console.warn(`[shadow] pooledMaxMismatch: hist=${mx} chanMax=${chanMax} — reporting chanMax`);
            const pc = (i:number)=> pixTotal?+((chNz[i]/pixTotal)*100).toFixed(4):0;
            const pcR=pc(0), pcG=pc(1), pcB=pc(2), pcA=pc(3);
            // global diffPct = mean(R,G,B,A) — consistent with per-channel pixel-rate definition (R2 Step 1 audit fix)
            divergence={ max:chanMax, histMax:mx, pooledMaxMismatch: mx!==chanMax, p50:q(0.5), p95:q(0.95), p99:q(0.99), mean:+(sum/total).toFixed(3) as unknown as number, diffPct: +(((pcR+pcG+pcB+pcA)/4)).toFixed(4) as unknown as number, sampledTiles: tiles.length };
            perChannel=(["R","G","B","A"] as const).map((ch,i)=>({ channel:ch, max:chMax[i], mean: pixTotal?+(chSum[i]/pixTotal).toFixed(3) as unknown as number:0, diffPct: pc(i) }));
            const mkB=(b:{pixelCount:number;maxDiff:number;sumDiff:number})=>({ pixelCount:b.pixelCount, maxDiff:b.maxDiff, meanDiff:b.pixelCount?+(b.sumDiff/b.pixelCount).toFixed(3):0 });
            spatial={ differingPixels:pixelDiffCount, differingChannels:nz, bbox: isFinite(minX)?{minX,minY,maxX,maxY}:null,
              alphaBuckets:[ {range:"0-63",...mkB(ab[0])},{range:"64-127",...mkB(ab[1])},{range:"128-191",...mkB(ab[2])},{range:"192-255",...mkB(ab[3])} ],
              edgeBuckets:[ {kind:"transparent",...mkB(eb.transparent)},{kind:"partial",...mkB(eb.partial)},{kind:"opaque",...mkB(eb.opaque)} ] };
          }
       } catch {}
       divergenceMs = performance.now() - tDiv0;
     }
   }
   const tMem0 = performance.now();
   let memPeak: number|null=null;
   let memBaseline: {current:number;peak:number}|null=null;
   let memAfter: {current:number;peak:number}|null=null;
   try {
     const m=await invoke("paint_parity_mem") as { current_mb:number; peak_mb:number }|null;
     memPeak=m?m.peak_mb:null;
     memAfter = m ? { current: m.current_mb, peak: m.peak_mb } : null;
     // baseline is not available inside this function (needs caller), so leave null here; autorun will fill memBaseline
     memBaseline = null;
   } catch {}
   const memMs = performance.now() - tMem0;
   // T0→T8 vs T1→T3 distinction: productionLikeT0toT8Ms is T0→rAF (excludes diagnostic); totalMs includes diagnostic
   const totalMs = +(performance.now()-p.t0).toFixed(3);
    const jsOverheadMs = Math.max(0, totalMs - (tipRegMs + wall + divergenceMs + memMs + rAFMs) - computeMs);
    // Stage beacon: diagnostics (divergence + tiles_for_keys + memory IPC) fully complete — record about to log.
    try { (window as unknown as Record<string, unknown>).__photrezShadowStage = { stage: "diagnostics-complete" }; } catch {}
    // ── R2 Step2 tip-blend fidelity probe (DEV-only, approved single measurement) ──
    // A = tip bytes sent to Rust · B = real Chrome drawImage→getImageData on isolated white canvas (1:1)
    // C = JS emulation of Rust fixed-point src-over (mul_div_255) applied to A over white
    let tipBlendProbe: ShadowRecord["tipBlendProbe"] = null;
    try {
      if (p.tip && tb.data.length) {
        const sz = p.tip.width;
        const rep = new OffscreenCanvas(sz, sz);
        const rctx = rep.getContext("2d", { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D;
        rctx.fillStyle = "#ffffff"; rctx.fillRect(0, 0, sz, sz);
        rctx.drawImage(p.tip, 0, 0);
        const B = rctx.getImageData(0, 0, sz, sz).data;
        const A = tb.data;
        const md = (c: number, a: number) => ((c * a + 127) / 255) | 0;
        const Cb = new Uint8ClampedArray(A.length);
        for (let i = 0; i < A.length; i += 4) {
          const sa = A[i + 3], inv = 255 - sa;
          Cb[i]     = Math.min(255, md(A[i],     sa) + md(255, inv));
          Cb[i + 1] = Math.min(255, md(A[i + 1], sa) + md(255, inv));
          Cb[i + 2] = Math.min(255, md(A[i + 2], sa) + md(255, inv));
          Cb[i + 3] = 255;
        }
        const diff = (X: Uint8ClampedArray | Uint8Array, Y: Uint8ClampedArray | Uint8Array) => {
          let mx = 0, sum = 0; const chMx = [0, 0, 0, 0], chSum = [0, 0, 0, 0];
          for (let i = 0; i < X.length; i++) {
            const d = Math.abs(X[i] - Y[i]); const ch = i & 3;
            if (d > mx) mx = d; sum += d; chSum[ch] += d; if (d > chMx[ch]) chMx[ch] = d;
          }
          return { max: mx, mean: +(sum / X.length).toFixed(3), chMax: chMx, chMean: chSum.map((v) => +(v / (X.length / 4)).toFixed(3)) };
        };
        const ab = diff(A, B), bc = diff(B, Cb), ac = diff(A, Cb);
        // ── Stage discrimination (R2 Step2 forensic): which stage breaks A→blend parity? ──
        // Stage-1: tip → transparent GPU-scratch (default attrs, like production cachedTileScratch)
        const sc2 = new OffscreenCanvas(sz, sz);
        const sc2ctx = sc2.getContext("2d") as OffscreenCanvasRenderingContext2D;
        sc2ctx.clearRect(0, 0, sz, sz);
        sc2ctx.drawImage(p.tip, 0, 0);
        const Sread = sc2ctx.getImageData(0, 0, sz, sz).data;
        // Stage-2: scratch → software white surface (like PaintTileSurface willReadFrequently)
        const surf = new OffscreenCanvas(sz, sz);
        const surfCtx = surf.getContext("2d", { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D;
        surfCtx.fillStyle = "#ffffff"; surfCtx.fillRect(0, 0, sz, sz);
        surfCtx.drawImage(sc2, 0, 0);
        const D = surfCtx.getImageData(0, 0, sz, sz).data;
        // Pure storage identity WITHOUT blend: putImageData(A) → getImageData
        const st = new OffscreenCanvas(sz, sz);
        const stc = st.getContext("2d", { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D;
        stc.putImageData(new ImageData(new Uint8ClampedArray(A), sz, sz), 0, 0);
        const E = stc.getImageData(0, 0, sz, sz).data;
        const sA = diff(Sread, A), dc2 = diff(D, Cb), db = diff(D, B), ea = diff(E, A);
        tipBlendProbe = { abMax: ab.max, abMean: ab.mean, bcMax: bc.max, bcMean: bc.mean, acMax: ac.max, acMean: ac.mean,
          abPerChannel: ab.chMax, bcPerChannel: bc.chMax, acPerChannel: ac.chMax,
          abChMean: ab.chMean, bcChMean: bc.chMean, acChMean: ac.chMean,
          sReadVsAMax: sA.max, sReadVsAMean: sA.mean,
          dVsCMax: dc2.max, dVsCMean: dc2.mean, dVsCPerChannel: dc2.chMax,
          dVsBMax: db.max, dVsBMean: db.mean,
          eVsAMax: ea.max, eVsAMean: ea.mean };
        console.info("[shadow] tipBlendProbe", JSON.stringify(tipBlendProbe));
      }
    } catch (e) { console.warn("[shadow] tipBlendProbe failed", e); }
    // ── R2 Step2 relays ──
    let scratchForensics: Record<string, unknown> | null = null;
    let parityDump: Record<string, unknown> | null = parityDumpEarly;
    try {
      const w2 = window as unknown as Record<string, unknown>;
      if (w2.__photrezScratchForensics) {
        scratchForensics = w2.__photrezScratchForensics as Record<string, unknown>;
        delete w2.__photrezScratchForensics;
      }
    } catch {}
   const rec: ShadowRecord = {
     ts: new Date().toISOString(), w:p.w, h:p.h, brush:p.brush, dabs:p.dabs.length,
     totalMs, t0toVisibleMs,
     productionLikeT0toT8Ms, diagnosticDivergenceMs: divergenceMs,
     rust: { prepMs:+(res!.prep_us/1000).toFixed(3) as unknown as number, rasterMs:+(res!.raster_us/1000).toFixed(3) as unknown as number, patchMs:+(res!.patchgen_us/1000).toFixed(3) as unknown as number, transportMs:+transportMs.toFixed(3) as unknown as number, wallMs:+wall.toFixed(3) as unknown as number },
     tsDigest, rustDigest: res!.digest, digestEqual, tileCount: res!.tile_count, changedBytes: res!.changed_bytes,
      divergence, perChannel, spatial,       wouldBeVersion: null, memRustPeakMb: memPeak, memBaselineMb: memBaseline, memAfterMb: memAfter, tipBlendProbe, scratchForensics, parityBrush: tip ? effBrush : null, parityDump, rustTiles: rustTilesRelay, canonicalTip: ladderForensics, breakdown: { tipRegMs: +tipRegMs.toFixed(3), invokeWallMs: +wall.toFixed(3), divergenceMs: +divergenceMs.toFixed(3), memMs: +memMs.toFixed(3), rAFMs: +rAFMs.toFixed(3), jsOverheadMs: +jsOverheadMs.toFixed(3), shadowEntryGapMs, getInvokeMs, tipReadbackMs, tipArrayFromMs, tipRegisterIpcMs, tsDigestHashMs },
   };
  log(rec);
  return rec;
}

function ensureShadowBadge() {
  if (typeof document === 'undefined') return;
  let el = document.getElementById('photrez-shadow-badge');
  if (!el) {
    el = document.createElement('div');
    el.id = 'photrez-shadow-badge';
    el.style.cssText = 'position:fixed;bottom:8px;right:8px;z-index:9999;padding:6px 10px;border-radius:9999px;font:12px/1.2 ui-sans-serif,system-ui;background:#0f172a;color:#fff;box-shadow:0 2px 8px rgba(0,0,0,.3);opacity:.95;pointer-events:auto;cursor:pointer';
    el.title = 'Click to copy window.__photrezShadowLog to clipboard';
    el.addEventListener('click', () => {
      const w = window as unknown as Record<string, unknown>;
      const arr = w.__photrezShadowLog as unknown[] | undefined;
      const txt = JSON.stringify(arr ?? [], null, 2);
      navigator.clipboard?.writeText(txt).then(()=> { const prev=el!.textContent; el!.textContent='Copied!'; setTimeout(()=> el && (el.textContent=prev), 1200); }).catch(()=> {});
    });
    document.body.appendChild(el);
  }
  return el as HTMLDivElement;
}
function setShadowBadge(text: string, ok: boolean | null) {
  const el = ensureShadowBadge();
  if (!el) return;
  el.textContent = text;
  el.style.background = ok === null ? '#0f172a' : ok ? '#16a34a' : '#dc2626';
  el.style.display = 'block';
}
function log(rec: ShadowRecord) {
  const w = window as unknown as Record<string, unknown>;
  const arr = (w.__photrezShadowLog as ShadowRecord[] | undefined) ?? [];
  arr.push(rec);
  if (arr.length>50) arr.shift();
  w.__photrezShadowLog = arr;
  const tag = rec.digestEqual ? "EQUAL" : "DIFF";
  const ok = !rec.error;
  console.info(`[shadow] ${tag} brush=${rec.brush} dabs=${rec.dabs} tiles=${rec.tileCount} ts=${rec.tsDigest.slice(0,8)} rust=${rec.rustDigest.slice(0,8)} Rust=${rec.rust.rasterMs}+${rec.rust.patchMs}ms transport=${rec.rust.transportMs}ms totalT0=${rec.totalMs}ms wouldBeV=${rec.wouldBeVersion} memPeak=${rec.memRustPeakMb} div=${rec.divergence?`${rec.divergence.max}/${rec.divergence.p99}`:"—"} ${rec.error?`ERR:${rec.error}`:''}`, rec);
  // Visible indicator so user knows it ran and succeeded/failed without opening DevTools
  setShadowBadge(`Shadow: ${tag} ${rec.brush}px ${rec.tileCount} tiles ${ok?'✓':'✗'}`, ok);
  // Also keep a 3s pulse then show summary
  setTimeout(() => {
    const all = (window as unknown as Record<string, unknown>).__photrezShadowLog as ShadowRecord[] | undefined;
    if (all && all.length) {
      const errs = all.filter(r=> !!r.error).length;
      setShadowBadge(`Shadow: ${all.length} runs ${errs?` ${errs} ERR`:''} (click to copy)`, errs===0);
    }
  }, 3000);
}

// ── Real T0→T8 autorun via production pointer path (no synthetic PaintTileSurface) ──
// Reuses existing production `pointerdown→pointermove→pointerup→commitBrushStroke→Shadow`
// dispatching real PointerEvents to the canvas. No second brush pipeline.
type WorkloadSpec = { id:string; W:number; H:number; brush:number; hardness:number; dabs:number; repeats?:number; concentric?:boolean };

// Heavy workloads for shadow/parity autorun validation.
// Driven via the REAL production pointer path (not synthetic runShadowForCommit).
export const HEAVY_WORKLOADS: WorkloadSpec[] = [
  { id:"shadow-3000x10-stress", W:6912, H:3888, brush:3000, hardness:0.8, dabs:10 },
  { id:"shadow-4Kplus-dense", W:4096, H:4096, brush:512, hardness:0.8, dabs:300 },
];

// Controlled depth probe v2 (R2, APPROVED): spacing-exact concentric ladder via MICRO-LOOPS.
// brush 256 → spacing = round(256×0.10) = 26 px (brushTipMask.getBrushDabSpacing).
// Each loop = two 13px moves C→(C+13)→C: cumulative arc crosses the 26px boundary exactly AT C,
// producing ONE pixel-identical emission at C per loop (producer carry verified: 13↔0 oscillation).
// Cursor finishes at C → terminal dab self-suppresses. Session = 1 anchor + (N−1) loops = N dabs,
// deterministic per EVENT COUNT (no wall-clock dependence); sync dispatch keeps DAB_HOLD_MS inert.
const LADDER_HALF_PX = 13;
const PROBE_WORKLOADS: WorkloadSpec[] = [1,2,4,8].map((n)=>({ id:`ladder-depth-${n}`, W:1024, H:1024, brush:256, hardness:0.8, dabs:n, repeats:1, concentric:true }));

// Serpentine stroke in DOCUMENT space; ~1px spacing → dab count ≈ length(px) (production interpolates ~1px).
function genStroke(W:number,H:number,brush:number,targetDabs:number): {x:number;y:number}[] {
  const m = Math.ceil(brush/2)+4;
  const xx0=m, yy0=m, xx1=W-m, yy1=H-m;
  const pts: {x:number;y:number}[] = [];
  let x = xx0+(xx1-xx0)/2, y = yy0+(yy1-yy0)/2, dir=1;
  const total = Math.max(2, targetDabs|0);
  for (let i=0;i<total;i++) {
    pts.push({x:Math.round(x), y:Math.round(y)});
    let nx = x + dir*1;
    if (nx>xx1){ dir=-1; nx=x; y+=1; } else if (nx<xx0){ dir=1; nx=x; y+=1; }
    if (y>yy1) y=yy0;
    x=nx;
  }
  return pts;
}

async function runOneRealPointerupShadow(workloads: WorkloadSpec[] = PROBE_WORKLOADS): Promise<ShadowRecord[]> {
  // minimal in-app pointer dispatch; no new engine/history infra
  // R2 Step 1 fix: poll ONLY for the editor context here. The canvas/engine can ONLY exist once a
  // document is active, and each case creates its own baseline doc below — requiring canvas at this
  // point would fail before any baseline doc is created (the prior regression).
  const w = window as unknown as Record<string, unknown>;
  const eDeadline = performance.now() + 8000;
  let ed = (w.__photrezEditor as
    | { workspace: { getActiveEngine(): unknown; getActiveHistory(): unknown; getActiveDocumentId(): unknown; addDocument: (s: unknown) => void; removeDocument: (id: string) => void }; activeTool: () => string; setActiveTool: (t: string) => void; setBrushSize: (n: number) => void } | undefined) ?? undefined;
  while (performance.now() < eDeadline) {
    ed = (w.__photrezEditor as
      | { workspace: { getActiveEngine(): unknown; getActiveHistory(): unknown; getActiveDocumentId(): unknown; addDocument: (s: unknown) => void; removeDocument: (id: string) => void }; activeTool: () => string; setActiveTool: (t: string) => void; setBrushSize: (n: number) => void } | undefined) ?? undefined;
    if (ed) break;
    await new Promise<void>((r) => setTimeout(r, 100));
  }
  try { localStorage.setItem("photrez.rustShadow", "1"); } catch {}
  if (!ed) {
    const rec: ShadowRecord = { ts: new Date().toISOString(), w: 0, h: 0, brush: 0, dabs: 0, totalMs: 0, t0toVisibleMs: null, rust: { prepMs: 0, rasterMs: 0, patchMs: 0, transportMs: 0, wallMs: 0 }, tsDigest: "", rustDigest: "", digestEqual: false, tileCount: 0, changedBytes: 0, divergence: null, perChannel: null, spatial: null, wouldBeVersion: null, memRustPeakMb: null, error: "real autorun: editor not ready" };
    (rec as unknown as Record<string, unknown>).caseId = "real-brush-pointerup";
    return [rec];
  }
  const mk = (type: string, x: number, y: number) => new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, pointerId: 1, pointerType: "mouse", button: 0, buttons: type === "pointerup" ? 0 : 1, isPrimary: true });
  const raf = () => new Promise<void>((r) => requestAnimationFrame(() => r()));
  // R2 Step 1 bounded repeatability: 4 identical real commits, EACH from a fresh blank baseline.
  // Reset baseline doc before each case via existing WorkspaceManager (removeDocument + createBlankDocument + addDocument) — no new infra.
  const { WorkspaceManager } = await import("@/engine/workspace");
  const ws = ed.workspace as unknown as { addDocument: (s: unknown) => void; removeDocument: (id: string) => void };
    const mkBlank = (tag: string, W: number, H: number): string => {
      const id = `auto-shadow-${tag}-${Date.now()}`;
      const sess = (WorkspaceManager as unknown as { createBlankDocument: (id: string, n: string, w: number, h: number, o?: unknown) => unknown }).createBlankDocument(id, "AutoShadow", W, H, { backgroundColor: "white" });
      ws.addDocument(sess);
      return id;
    };
  const out: ShadowRecord[] = [];
  let prevAutoId: string | null = null;
  for (let wi = 0; wi < workloads.length; wi++) {
    const spec = workloads[wi];
    const reps = spec.repeats ?? 1;
    for (let rep = 0; rep < reps; rep++) {
      // Progress markers — bounded, observable execution (no silent/stale runs)
      const label = `heavy ${wi + 1}/${workloads.length}`;
      console.info(`[shadow] ${label} started: ${spec.id} ${spec.W}x${spec.H} brush=${spec.brush} targetDabs=${spec.dabs}`);
      setShadowBadge(`${label}: ${spec.id} starting…`, null);
      try { (w as unknown as Record<string, unknown>).__photrezShadowStage = { stage: "idle", caseId: spec.id }; } catch {}
      // reset baseline: drop previous auto doc, create fresh blank doc at workload size
      if (prevAutoId) { try { ws.removeDocument(prevAutoId); } catch {} }
      prevAutoId = mkBlank(`${spec.id}-${rep}`, spec.W, spec.H);
      // re-ensure brush tool + size + hardness (tool may reset on doc switch)
      try { if (typeof ed.setActiveTool === "function" && ed.activeTool() !== "brush") ed.setActiveTool("brush"); } catch {}
      try { if (typeof (ed as unknown as { setBrushSize?: (n:number)=>void }).setBrushSize === "function") (ed as unknown as { setBrushSize:(n:number)=>void }).setBrushSize(spec.brush); } catch {}
      try { const sh = (ed as unknown as { setBrushHardness?: (n:number)=>void }).setBrushHardness; if (typeof sh === "function") sh(spec.hardness); } catch {}
      // Readiness validation: poll until canvas + active engine + active document are present.
      const cDeadline = performance.now() + 12000;
      let c: HTMLCanvasElement | null = null;
      while (performance.now() < cDeadline) {
        const eng = ed.workspace.getActiveEngine?.();
        const docId = ed.workspace.getActiveDocumentId?.();
        c = document.querySelector("canvas") as HTMLCanvasElement | null;
        if (!c) { try { c = (ed as unknown as { renderer?: { getCanvas?: () => HTMLCanvasElement | null } })?.renderer?.getCanvas?.() ?? null; } catch {} }
        if (c && eng && docId) break;
        await new Promise<void>((r) => setTimeout(r, 100));
      }
      const tag = `${spec.id}#${rep+1}`;
      if (!c) {
        const rec: ShadowRecord = { ts: new Date().toISOString(), w: spec.W, h: spec.H, brush: spec.brush, dabs: 0, totalMs: 0, t0toVisibleMs: null, rust: { prepMs: 0, rasterMs: 0, patchMs: 0, transportMs: 0, wallMs: 0 }, tsDigest: "", rustDigest: "", digestEqual: false, tileCount: 0, changedBytes: 0, divergence: null, perChannel: null, spatial: null, wouldBeVersion: null, memRustPeakMb: null, error: `${tag}: canvas/engine not ready after baseline reset` };
        (rec as unknown as Record<string, unknown>).caseId = spec.id;
        out.push(rec);
        continue;
      }
      const rect = c.getBoundingClientRect();
      const sx = rect.width / spec.W, sy = rect.height / spec.H;
      const toClient = (p:{x:number;y:number}) => ({ x: rect.left + p.x*sx, y: rect.top + p.y*sy });
      const beforeLen = ((w.__photrezShadowLog as unknown[]) ?? []).length;
      // REAL production path: dispatch document-space stroke (mapped to client) → onPaintStroke → commitBrushStroke(_t0) → Shadow
      let pathLog = "";
      let eventCount = 0;
      if (spec.concentric) {
        // Spacing-exact concentric ladder via MICRO-LOOPS C→(C+13)→C: cumulative arc crosses
        // the 26px boundary exactly AT C → one pixel-identical emission at C per loop;
        // cursor ends at C → terminal dab self-suppresses.
        const cx = Math.floor(spec.W / 2), cy = Math.floor(spec.H / 2);
        const ctr = toClient({ x: cx, y: cy });
        const coords: string[] = [`down@(${cx},${cy})`];
        c.dispatchEvent(mk("pointerdown", ctr.x, ctr.y));
        await raf();
        let ev = 0;
        for (let i = 1; i < spec.dabs; i++) {
          const ox = cx + LADDER_HALF_PX;
          const oP = toClient({ x: ox, y: cy });
          c.dispatchEvent(mk("pointermove", oP.x, oP.y)); // synchronous burst — hold timer cannot interleave
          coords.push(`m${++ev}@(${ox},${cy})`);
          const bP = toClient({ x: cx, y: cy });
          c.dispatchEvent(mk("pointermove", bP.x, bP.y));
          coords.push(`m${++ev}@(${cx},${cy})`);
        }
        c.dispatchEvent(mk("pointerup", ctr.x, ctr.y));
        coords.push(`up@(${cx},${cy})`);
        eventCount = 2 + 2 * (spec.dabs - 1);
        pathLog = coords.join(" ");
      } else {
        const pts = genStroke(spec.W, spec.H, spec.brush, spec.dabs);
        const p0 = toClient(pts[0]);
        c.dispatchEvent(mk("pointerdown", p0.x, p0.y));
        await raf();
        for (let i = 1; i < pts.length; i++) {
          const pc = toClient(pts[i]);
          c.dispatchEvent(mk("pointermove", pc.x, pc.y));
          await raf();
        }
        const pend = toClient(pts[pts.length - 1]);
        c.dispatchEvent(mk("pointerup", pend.x, pend.y));
        eventCount = pts.length + 1;
        pathLog = `serpentine ${pts.length}pts down@(${pts[0].x},${pts[0].y})`;
      }
      // wait for Shadow record (fire-and-forget runShadowForCommit). Heavy commits can take seconds.
      // Bounded: emits T0→T8 + diagnostics-complete markers; on timeout exports a diagnostic failure record and stops this case.
      const waitDeadline = performance.now() + 30000;
      let got: ShadowRecord | null = null;
      let t8Seen = false;
      while (performance.now() < waitDeadline) {
        const st = (w.__photrezShadowStage as { stage?: string; t0toVisibleMs?: number } | undefined);
        if (!t8Seen && st?.stage === "t8") {
          t8Seen = true;
          console.info(`[shadow] ${label} T0→T8 captured: ${st.t0toVisibleMs}ms`);
          setShadowBadge(`${label}: T0→T8 ${st.t0toVisibleMs}ms`, null);
        }
        const arr = (w.__photrezShadowLog as ShadowRecord[] | undefined) ?? [];
        if (arr.length > beforeLen) {
          got = arr[arr.length - 1] as ShadowRecord;
          (got as unknown as Record<string, unknown>).caseId = spec.id;
          (got as unknown as Record<string, unknown>).note = `real production pointer path; workload ${spec.W}x${spec.H} brush ${spec.brush} hardness ${spec.hardness} target ${spec.dabs} dabs (actual ${got.dabs}); path(${eventCount} ev): ${pathLog}; spacing=round(${spec.brush}×0.10)=${Math.round(spec.brush*0.10)}px; FULL-CENSUS divergence (no tile caps); fresh blank baseline reset; T0=commitBrushStroke entry (_t0), excludes diagnostic`;
          console.info(`[shadow] ${label} diagnostics complete: dabs=${got.dabs} tiles=${got.tileCount} changedBytes=${got.changedBytes} divMax=${got.divergence?.max ?? "—"} err=${got.error ?? "none"}`);
          setShadowBadge(`${label}: diagnostics ✓ (${got.error ? "ERR" : got.digestEqual ? "EQUAL" : "DIFF"})`, !got.error);
          break;
        }
        await new Promise<void>((r) => setTimeout(r, 50));
      }
      if (!got) {
        console.warn(`[shadow] ${label} TIMEOUT after 30s (t8Seen=${t8Seen}) — stopping this case`);
        setShadowBadge(`${label}: TIMEOUT`, false);
      }
      if (got) { out.push(got); }
      else {
        const rec: ShadowRecord = { ts: new Date().toISOString(), w: spec.W, h: spec.H, brush: spec.brush, dabs: 0, totalMs: 0, t0toVisibleMs: null, rust: { prepMs: 0, rasterMs: 0, patchMs: 0, transportMs: 0, wallMs: 0 }, tsDigest: "", rustDigest: "", digestEqual: false, tileCount: 0, changedBytes: 0, divergence: null, perChannel: null, spatial: null, wouldBeVersion: null, memRustPeakMb: null, error: `${tag}: Shadow record did not appear after pointerup (timeout 30s, t8Seen=${t8Seen})` };
        (rec as unknown as Record<string, unknown>).caseId = spec.id;
        out.push(rec);
      }
    }
  }
  return out;
}

// ── Previously working automation restored: PHOTREZ_SHADOW_AUTO + paint_shadow_export (no CDP) ──
// This mirrors the parity harness `PHOTREZ_PARITY_AUTO` workflow that was verified to work
// without any browser/CDP attachment. The current Tauri process differs from the
// previously automatable one only by missing env var and --remote-debugging-port;
// restarting with PHOTREZ_SHADOW_AUTO=1 restores automated access via %TEMP% export.
if (import.meta.env.DEV && isTauriEnv()) {
  void (async () => {
    try {
      const invoke = await getInvoke();
      const shadowOn = await invoke("paint_shadow_autorun_enabled") as boolean;
      const parityOn = await invoke("paint_parity_autorun_enabled") as boolean;
      if (!shadowOn && !parityOn) return;
      // shadow autorun restores the previously working automation (parity harness used PHOTREZ_PARITY_AUTO + export to %TEMP% without CDP)
      // Now also runs when PHOTREZ_PARITY_AUTO=1 (existing env) so the current Tauri process can be reused without new launch flags
      setShadowBadge('Shadow autorun: starting…', null);
      console.log("[shadow] autorun: starting 4 workloads (3000×10, 4K+ dense, rapid burst, 10-commit)");
      // Lazy import to avoid cycle; reuse bench helpers for deterministic tip/dabs
      const { PaintTileSurface } = await import("@/lib/paint/paintTileSurface");
      const { buildTipCanvas } = await import("@/lib/paint/buildTipCanvas");
      // Deterministic helpers for reproducible dab sequences
      const lcg = (seed: number) => { let s = seed >>> 0; return () => { s = (s * 1664525 + 1013904223) >>> 0; return s / 0xffffffff; }; };
      const genDabs = (W:number,H:number,n:number,seed:number) => {
        const rnd = lcg(seed); const out:{x:number;y:number;alpha:number}[]=[];
        for(let i=0;i<n;i++) out.push({x: Math.floor(rnd()*W), y: Math.floor(rnd()*H), alpha:0.75});
        return out;
      };
      const cases: { id:string; W:number; H:number; brush:number; hardness:number; dabs:number; eraser:boolean; prepWhite:boolean }[] = [
        { id:"shadow-3000x10-stress", W:6912, H:3888, brush:3000, hardness:0.8, dabs:10, eraser:false, prepWhite:true },
        { id:"shadow-4Kplus-dense", W:4096, H:4096, brush:512, hardness:0.8, dabs:300, eraser:false, prepWhite:true },
        { id:"shadow-rapid-burst-1", W:2048, H:2048, brush:256, hardness:0.7, dabs:50, eraser:false, prepWhite:true },
        { id:"shadow-rapid-burst-2", W:2048, H:2048, brush:256, hardness:0.7, dabs:50, eraser:false, prepWhite:true },
        { id:"shadow-rapid-burst-3", W:2048, H:2048, brush:256, hardness:0.7, dabs:50, eraser:false, prepWhite:true },
      ];
      // ── Memory baseline BEFORE Shadow (Rust allocator, not RSS) ──
      let memBaseline: { current:number; peak:number } | null = null;
      try {
        await invoke("paint_parity_mem_reset");
        const m0 = await invoke("paint_parity_mem") as { current_mb:number; peak_mb:number };
        memBaseline = { current: m0.current_mb, peak: m0.peak_mb };
        console.log(`[shadow] mem baseline: current=${m0.current_mb} peak=${m0.peak_mb} MB (Rust allocator, not RSS)`);
      } catch {}
      const records: ShadowRecord[] = [];
      for (let idx=0; idx<cases.length; idx++) {
        const c = cases[idx];
        const dabs = genDabs(c.W,c.H,c.dabs,0x5eed+idx*7919);
        const tip = buildTipCanvas(c.brush, c.hardness);
        // Build TS after patches via PaintTileSurface — measure TS prep separately for reconciliation
        const tTsPrep0 = performance.now();
        const surf = new PaintTileSurface(c.W,c.H,null as unknown as never);
        const ctx = (surf as unknown as { context: CanvasRenderingContext2D }).context;
        ctx.fillStyle="white"; ctx.fillRect(0,0,c.W,c.H);
        if (!c.prepWhite) { ctx.fillStyle="rgb(30,120,220)"; ctx.fillRect(0,0,c.W,c.H); }
        ctx.globalCompositeOperation = c.eraser?"destination-out":"source-over";
        const r=c.brush/2; for(const d of dabs){ ctx.globalAlpha=d.alpha; (ctx as unknown as { drawImage:(a:unknown,b:number,c:number,d:number,e:number)=>void }).drawImage(tip as unknown as CanvasImageSource, Math.round(d.x-r), Math.round(d.y-r), c.brush, c.brush); } ctx.globalAlpha=1;
        const margin=Math.ceil(c.brush/2)+2;
        const x0=Math.max(0, Math.min(...dabs.map(d=>d.x))-margin);
        const y0=Math.max(0, Math.min(...dabs.map(d=>d.y))-margin);
        const x1=Math.min(c.W, Math.max(...dabs.map(d=>d.x))+margin);
        const y1=Math.min(c.H, Math.max(...dabs.map(d=>d.y))+margin);
        const { tilesInRect } = await import("@/lib/paint/paintTileSurface");
        const tiles = tilesInRect(Math.max(0,x0),Math.max(0,y0),Math.min(c.W,x1),Math.min(c.H,y1),c.W,c.H);
        const tsAfter = new Map<string, Uint8ClampedArray>();
        for (const t of tiles) {
          const im = ctx.getImageData(t.x,t.y,t.w,t.h);
          tsAfter.set(`${t.x/256},${t.y/256}`, new Uint8ClampedArray(im.data));
        }
        const tsPrepMs = performance.now() - tTsPrep0;
        const t0 = performance.now();
        const rec = await runShadowForCommit({ w:c.W, h:c.H, brush:c.brush, hardness:c.hardness, dabs, tip: tip as unknown as OffscreenCanvas, eraser:c.eraser, prepWhite:c.prepWhite, tsAfter, t0 });
        if (rec) {
          (rec as unknown as Record<string,unknown>).caseId=c.id;
          (rec as unknown as Record<string,unknown>).tsPrepMs = +tsPrepMs.toFixed(3);
          if (idx===0) (rec as unknown as Record<string,unknown>).memBaseline = memBaseline;
          records.push(rec);
          setShadowBadge(`Shadow autorun: ${records.length}/15 ${rec.error?'ERR':rec.digestEqual?'EQUAL':'DIFF'}`, !rec.error);
        }
        if (c.id.startsWith("shadow-rapid")) await new Promise(res=> setTimeout(res, 50));
      }
      // 10-commit memory-growth sequence (same 3000×10 workload, 10 sequential shadows)
      for (let i=0;i<10;i++) {
        const c={ W:2048,H:2048,brush:256,hardness:0.7,dabs:50,eraser:false,prepWhite:true };
        const dabs=genDabs(c.W,c.H,c.dabs,0x6000+i);
        const tip=buildTipCanvas(c.brush,c.hardness);
        const surf=new PaintTileSurface(c.W,c.H,null as unknown as never);
        const ctx2=(surf as unknown as {context:CanvasRenderingContext2D}).context;
        ctx2.fillStyle="white"; ctx2.fillRect(0,0,c.W,c.H);
        ctx2.globalCompositeOperation="source-over";
        const r2=c.brush/2; for(const d of dabs){ ctx2.globalAlpha=d.alpha; (ctx2 as unknown as {drawImage:(a:unknown,b:number,c:number,d:number,e:number)=>void}).drawImage(tip as unknown as CanvasImageSource, Math.round(d.x-r2), Math.round(d.y-r2), c.brush,c.brush);} ctx2.globalAlpha=1;
        const tsAfter2=new Map<string,Uint8ClampedArray>();
        const { tilesInRect: tir2 } = await import("@/lib/paint/paintTileSurface");
        const tiles2=tir2(0,0,c.W,c.H,c.W,c.H).slice(0,16);
        for(const t of tiles2){ const im=ctx2.getImageData(t.x,t.y,t.w,t.h); tsAfter2.set(`${t.x/256},${t.y/256}`, new Uint8ClampedArray(im.data));}
        const rec2=await runShadowForCommit({ w:c.W,h:c.H,brush:c.brush,hardness:c.hardness,dabs, tip: tip as unknown as OffscreenCanvas, eraser:false, prepWhite:true, tsAfter: tsAfter2, t0: performance.now() });
        if(rec2) { records.push(rec2); setShadowBadge(`Shadow autorun: ${records.length}/15 ${rec2.error?'ERR':'✓'}`, !rec2.error); }
      }
      // ── REQUIRED: one real pointerup through production path (T0=commitBrushStroke _t0) ──
      // This is the only T0→T8-valid record; synthetic above is pipeline-only
      let realRecs: ShadowRecord[] = [];
      try {
        console.log("[shadow] real autorun: dispatching production pointerup(s)…");
        realRecs = await runOneRealPointerupShadow();
        if (realRecs.length) {
          records.unshift(...realRecs);
          console.log("[shadow] real autorun: captured", realRecs.length, "real records");
          const r0 = realRecs[0];
          setShadowBadge(`Shadow real: ${r0.error ? 'ERR' : r0.digestEqual ? 'EQUAL' : 'DIFF'} ${r0.t0toVisibleMs ?? r0.totalMs}ms`, !r0.error);
        }
      } catch (e) { console.warn("[shadow] real autorun failed (expected if no document/canvas):", e); }
      const json = JSON.stringify({ records, generatedAt: new Date().toISOString(), note: "PHOTREZ_SHADOW_AUTO: records[0]=real pointerup T0→T8 (production path), rest=synthetic pipeline-only — TS canonical, Rust candidate" });
      const outPath = await invoke("paint_shadow_export", { resultJson: json }) as string;
      const errs = records.filter(r=> !!r.error).length;
      setShadowBadge(`Shadow autorun: ${records.length} done ${errs?`${errs} ERR`: '✓'} (real ${realRecs.length? (realRecs[0].error?'ERR':'✓') : '—'} + ${records.length-realRecs.length} synth)`, errs===0);
      console.log("[shadow] autorun: exported to", outPath, `records=${records.length} errs=${errs} real=${realRecs[0]?.t0toVisibleMs ?? realRecs[0]?.totalMs ?? '—'}ms`);
    } catch (e) {
      console.error("[shadow] autorun failed", e);
      try { const inv=await getInvoke(); await inv("paint_shadow_export", { resultJson: JSON.stringify({ error: String(e) }) }); } catch {}
    }
  })();
}

// ── Auto one-shot real T0→T8 on plain `bun tauri dev` (no env, no manual) ──
// Runs once per launch, after editor/document/canvas ready. Hard timeout, never hangs.
// Keeps synthetic gated behind PHOTREZ_SHADOW_AUTO; this path is the required single real measurement.
if (import.meta.env.DEV && isTauriEnv()) {
  void (async () => {
    // Opt-in only: run the one-shot real T0→T8 measurement ONLY when the dev
    // autorun is explicitly enabled via PHOTREZ_SHADOW_AUTO / PHOTREZ_PARITY_AUTO.
    // Plain `bun tauri dev` stays clean so Rust-pixel runtime validation is deterministic.
    try {
      const invoke = await getInvoke();
      const shadowOn = await invoke("paint_shadow_autorun_enabled") as boolean;
      const parityOn = await invoke("paint_parity_autorun_enabled") as boolean;
      if (!shadowOn && !parityOn) return;
    } catch {
      return;
    }
    const WIN = window as unknown as Record<string, unknown>;
    if (WIN.__photrezRealOneShotDone) return;
    WIN.__photrezRealOneShotDone = true as unknown as string;
    // wait for actual editor readiness — not immediate at import (fixes earlier lifecycle failure)
    // outer gate: wait only for __photrezEditor, then delegate to helper (helper owns doc/canvas recovery)
    let lastStage = "init";
    const outerDeadline = performance.now() + 20000;
    while (performance.now() < outerDeadline) {
      const ed = WIN.__photrezEditor as { workspace?: unknown } | undefined;
      const hasEditor = !!ed;
      lastStage = `hasEditor=${hasEditor}`;
      if (hasEditor) break;
      await new Promise<void>((r) => setTimeout(r, 250));
    }
    const hasEditorNow = !!(WIN.__photrezEditor as { workspace?: unknown } | undefined);
    if (!hasEditorNow) {
      try {
        const invoke = await getInvoke();
        const diag = { caseId: "real-brush-pointerup", error: "one-shot timeout: __photrezEditor not ready", lastStage, t0toVisibleMs: null, totalMs: null };
        await invoke("paint_shadow_export", { resultJson: JSON.stringify({ records: [diag], generatedAt: new Date().toISOString(), note: "one-shot real T0→T8 timeout — __photrezEditor not reached" }) });
        console.warn("[shadow] one-shot real: timeout", lastStage);
        setShadowBadge("Shadow real: timeout", false);
      } catch {}
      return;
    }
    // delegate — helper handles missing doc/canvas/engine (auto-create blank doc + internal poll, hard timeout inside helper)
    setShadowBadge("Shadow one-shot: running real pointerup…", null);
    console.log("[shadow] one-shot real: hasEditor true — delegating to helper (helper owns doc/canvas recovery)", lastStage);
    let recs: ShadowRecord[] = [];
    try { recs = await runOneRealPointerupShadow(); } catch (e) { console.warn("[shadow] one-shot real dispatch failed", e); }
    // export array of repeated real records (R2 Step 1 bounded repeatability) to same %TEMP% path — agent polls this file
    try {
      const invoke = await getInvoke();
      const payload = recs.length
        ? { records: recs, generatedAt: new Date().toISOString(), note: `one-shot spacing-exact concentric ladder x${recs.length} (R2 APPROVED probe): brush256 spacing26px, legs C±13px @1024² white baseline each case, N=1/2/4/8 via real producer spacing/carry (pixel-identical emissions at C, deterministic per event count, hold-timer inert), FULL-CENSUS divergence, pooledMax guard, timestamp attribution in breakdown; T0=commitBrushStroke entry → Rust candidate → TS canonical requestRender → rAF` }
        : { records: [{ caseId: "real-brush-pointerup", error: "dispatch returned empty", lastStage }], generatedAt: new Date().toISOString() };
      const out = await invoke("paint_shadow_export", { resultJson: JSON.stringify(payload) });
      const last = recs[recs.length - 1];
      console.info("[shadow] export complete:", out, `${recs.length} records`);
      setShadowBadge(`Shadow one-shot: ${recs.length} commits ${last?.error ? "ERR" : last?.digestEqual ? "EQUAL" : "DIFF"} ${last?.t0toVisibleMs ?? last?.totalMs ?? "—"}ms`, !last?.error);
    } catch (e) { console.error("[shadow] one-shot export failed", e); }
  })();
}

// ── C3 seam helpers (DEV/flag-gated; reused by canonical commit path) ──

/** Apply Rust tile patches onto a surface 2D context via putImageData (existing seam). */
export interface PutImageDataTarget {
  putImageData(img: { width: number; height: number; data: Uint8ClampedArray }, x: number, y: number): void;
}
export function applyRustTilesToSurface(
  ctx: PutImageDataTarget,
  tiles: { x: number; y: number; w: number; h: number; data: ArrayLike<number> }[],
  ImageDataCtor?: new (data: Uint8ClampedArray, w: number, h: number) => { width: number; height: number; data: Uint8ClampedArray },
): void {
  const Ctor = ImageDataCtor ?? (globalThis as unknown as { ImageData: new (d: Uint8ClampedArray, w: number, h: number) => { width: number; height: number; data: Uint8ClampedArray } }).ImageData;
  for (const t of tiles) {
    const arr = new Uint8ClampedArray(t.w * t.h * 4);
    for (let i = 0; i < arr.length; i++) arr[i] = t.data[i];
    ctx.putImageData(new Ctor(arr, t.w, t.h), t.x, t.y);
  }
}

// C5.2: derived-cache epoch validation + rehydration from the Rust canonical buffer.
export interface PaintCacheLike {
  context: PutImageDataTarget;
  pixelEpoch: number;
  /** C5.3-A: optional `DocumentVersion` mirror for the authoritative history cursor. */
  pixelVersion?: number;
}

/** Current canonical epoch for (docId, layerId), or null when Rust has no pixel
 *  storage for that layer yet (e.g. before the Rust init path has seeded it). */
export async function getRustEpoch(docId: string, layerId: string): Promise<number | null> {
  try {
    const invoke = await getInvoke();
    const res = await invoke("rust_pixels_get_epoch", { docId, layerId });
    return typeof res === "number" ? res : null;
  } catch {
    return null;
  }
}

/** Rebuild a derived TS paint-surface cache from the Rust canonical layer when
 *  its cached epoch is stale. Returns true if a rehydration was performed.
 *  No-ops (false) when Rust has no storage yet, or epochs already match. */
export async function rehydratePaintSurfaceFromRust(
  docId: string,
  layerId: string,
  surface: PaintCacheLike,
): Promise<boolean> {
  const rustEpoch = await getRustEpoch(docId, layerId);
  if (rustEpoch == null) return false;
  if (surface.pixelEpoch === rustEpoch) return false;
  try {
    const invoke = await getInvoke();
    const tiles = (await invoke("rust_pixels_snapshot_layer", { docId, layerId })) as
      | { x: number; y: number; w: number; h: number; data: ArrayLike<number> }[]
      | null;
    if (!tiles || tiles.length === 0) return false;
    applyRustTilesToSurface(surface.context, tiles);
    surface.pixelEpoch = rustEpoch;
    return true;
  } catch {
    return false;
  }
}

/** True when every pixel of the byte buffer is opaque white (fresh blank base). */
export function isPristineOpaqueWhite(bytes: ArrayLike<number>): boolean {
  for (let i = 0; i < bytes.length; i += 4) {
    if (bytes[i] !== 255 || bytes[i + 1] !== 255 || bytes[i + 2] !== 255 || bytes[i + 3] !== 255) return false;
  }
  return true;
}
