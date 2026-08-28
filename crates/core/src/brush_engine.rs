// SPDX-License-Identifier: AGPL-3.0-or-later
// Brush dab producer (Fase 3 package 1): the stroke spacing/carry state
// machine ported 1:1 from apps/desktop/src/components/editor/brushTipMask.ts
// (interpolateDabs + getBrushDabSpacing). f64 internals match JS Number
// semantics so TS and Rust producers produce identical dab sequences.
// Readout discipline: dab_view() returns a Float64Array view into wasm memory
// that is only valid until the next begin()/update() call on this engine
// (heap may grow on reallocation) - consume it immediately, never retain.
use js_sys::Float64Array;
use wasm_bindgen::prelude::*;

/// spacing = max(1, round(size * 0.10)) - identical to getBrushDabSpacing.
#[wasm_bindgen]
pub fn brush_dab_spacing(size: f64) -> f64 {
    (size * 0.10).round().max(1.0)
}

#[wasm_bindgen]
pub struct BrushStrokeEngine {
    last_x: f64,
    last_y: f64,
    spacing: f64,
    carry: f64,
    started: bool,
    dabs: Vec<f64>, // x0,y0,x1,y1,... cleared per update
}

#[wasm_bindgen]
impl BrushStrokeEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(size: f64) -> BrushStrokeEngine {
        BrushStrokeEngine {
            last_x: 0.0,
            last_y: 0.0,
            spacing: brush_dab_spacing(size),
            carry: 0.0,
            started: false,
            dabs: Vec::new(),
        }
    }

    /// Update spacing when brush size changes mid-stroke (carry persists,
    /// matching the TS session behavior).
    pub fn set_size(&mut self, size: f64) {
        self.spacing = brush_dab_spacing(size);
    }

    /// Begin/reposition the stroke anchor. Emits no dab (the first stamp is
    /// emitted by the caller, matching paintSession behavior).
    pub fn begin(&mut self, x: f64, y: f64) {
        self.last_x = x;
        self.last_y = y;
        self.started = true;
        self.dabs.clear();
    }

    /// Feed one pointer move. Returns the number of dabs produced; read them
    /// via dab_view() (2 * count f64s: x,y pairs). View dies on next call.
    pub fn update(&mut self, x: f64, y: f64) -> usize {
        self.dabs.clear();
        self.append_move(x, y)
    }

    /// Batch feed (per-frame shape): moves is flat [x0,y0,x1,y1,...]. Clears
    /// the dab buffer once, appends across all moves; returns total dabs.
    /// Read via dab_view() after the call. Amortizes the JS<->wasm boundary
    /// to one crossing per frame instead of per pointer event.
    pub fn update_batch(&mut self, moves: &[f64]) -> usize {
        self.dabs.clear();
        let mut total = 0;
        let mut i = 0;
        while i + 1 < moves.len() {
            total += self.append_move(moves[i], moves[i + 1]);
            i += 2;
        }
        total
    }

    fn append_move(&mut self, x: f64, y: f64) -> usize {
        if !self.started {
            self.last_x = x;
            self.last_y = y;
            self.started = true;
            return 0;
        }
        let dx = x - self.last_x;
        let dy = y - self.last_y;
        let distance = dx.hypot(dy);
        if distance <= 0.0 {
            return 0;
        }
        let before = self.dabs.len();
        let mut next = self.spacing - self.carry;
        while next <= distance + 0.0001 {
            let t = next / distance;
            self.dabs.push(self.last_x + dx * t);
            self.dabs.push(self.last_y + dy * t);
            next += self.spacing;
        }
        self.carry = distance - (next - self.spacing);
        self.last_x = x;
        self.last_y = y;
        (self.dabs.len() - before) / 2
    }

    /// Zero-copy view of the dabs from the most recent update(). Valid until
    /// the next begin()/update() on this engine.
    pub fn dab_view(&self) -> Float64Array {
        unsafe { Float64Array::view(&self.dabs) }
    }

    pub fn carry(&self) -> f64 {
        self.carry
    }

    pub fn spacing(&self) -> f64 {
        self.spacing
    }

    /// Test-only direct access (js_sys views cannot be indexed off-wasm32).
    #[cfg(test)]
    pub(crate) fn dab_slice(&self) -> &[f64] {
        &self.dabs
    }

    /// Test-only: seed carry to reproduce TS reference cases that inject
    /// values unreachable via natural chaining (carry > spacing never occurs
    /// in live sessions).
    #[cfg(test)]
    pub(crate) fn seed_carry(&mut self, c: f64) {
        self.carry = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference cases mirror apps/desktop/src/components/editor/__tests__/
    // brushTipMask.test.ts "interpolateDabs edge cases" + segment identity.

    fn dab(engine: &mut BrushStrokeEngine, x: f64, y: f64) -> Vec<(f64, f64)> {
        let n = engine.update(x, y);
        let v = engine.dab_slice();
        (0..n).map(|i| (v[i * 2], v[i * 2 + 1])).collect()
    }

    #[test]
    fn spacing_matches_ts_formula() {
        assert_eq!(brush_dab_spacing(100.0), 10.0);
        assert_eq!(brush_dab_spacing(1.0), 1.0);
        assert_eq!(brush_dab_spacing(2.0), 1.0);
        assert_eq!(brush_dab_spacing(400.0), 40.0);
    }

    #[test]
    fn zero_distance_no_dabs_carry_kept() {
        let mut e = BrushStrokeEngine::new(50.0);
        e.begin(10.0, 10.0);
        assert_eq!(dab(&mut e, 10.0, 10.0).len(), 0);
        assert_eq!(e.carry(), 0.0);
    }

    #[test]
    fn short_segment_pure_carry() {
        // from (0,0) to (3,0), spacing 10 -> no dab, carry = 3
        let mut e = BrushStrokeEngine::new(100.0); // spacing 10
        e.begin(0.0, 0.0);
        assert_eq!(dab(&mut e, 3.0, 0.0).len(), 0);
        assert!((e.carry() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn exact_multiple_three_dabs_carry_zero() {
        // 30px at spacing 10 -> dabs at 10,20,30, carry 0
        let mut e = BrushStrokeEngine::new(100.0);
        e.begin(0.0, 0.0);
        let d = dab(&mut e, 30.0, 0.0);
        assert_eq!(d.len(), 3);
        assert!((d[0].0 - 10.0).abs() < 1e-9);
        assert!((d[2].0 - 30.0).abs() < 1e-9);
        assert!(e.carry().abs() < 1e-9);
    }

    #[test]
    fn chained_carry_reduces_first_gap() {
        // 3px (carry 3), then 10px: first dab candidate at 10-3=7 (< 10)
        let mut e = BrushStrokeEngine::new(100.0);
        e.begin(0.0, 0.0);
        dab(&mut e, 3.0, 0.0);
        let d = dab(&mut e, 13.0, 0.0);
        assert_eq!(d.len(), 1);
        assert!((d[0].0 - 10.0).abs() < 1e-9);
        assert!((e.carry() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn injected_wrap_carry_matches_ts_case() {
        // TS reference: (0,0)->(30,0) spacing 10 carry 12 -> next=-2 (behind
        // anchor), dabs at t=-2/8.., carry = 30 - (38-10) = 2
        let mut e = BrushStrokeEngine::new(100.0);
        e.begin(0.0, 0.0);
        e.seed_carry(12.0);
        let d = dab(&mut e, 30.0, 0.0);
        assert_eq!(d.len(), 4);
        assert!((e.carry() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn diagonal_carry_matches_ts_reference() {
        // TS reference: segment (dx,dy)=(5,5), spacing 10, carry 8 ->
        // distance=sqrt(50), first dab at t=2/distance, carry = 5.07...
        let mut e = BrushStrokeEngine::new(100.0);
        e.begin(18.0, 0.0); // anchor anywhere; math is segment-relative
        e.seed_carry(8.0);
        dab(&mut e, 23.0, 5.0);
        assert!((e.carry() - 5.071067811865476).abs() < 1e-9);
    }

    #[test]
    fn segment_split_equals_direct() {
        // (0,0)->(15,0)->(30,0) with carry chaining == direct (0,0)->(30,0)
        let mut a = BrushStrokeEngine::new(100.0);
        a.begin(0.0, 0.0);
        let mut all = dab(&mut a, 15.0, 0.0);
        all.extend(dab(&mut a, 30.0, 0.0));

        let mut b = BrushStrokeEngine::new(100.0);
        b.begin(0.0, 0.0);
        let direct = dab(&mut b, 30.0, 0.0);

        assert_eq!(all.len(), direct.len());
        for (p, q) in all.iter().zip(direct.iter()) {
            assert!((p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9);
        }
        assert!((a.carry() - b.carry()).abs() < 1e-9);
    }

    #[test]
    fn batch_matches_per_move() {
        let moves = [3.0, 0.0, 13.0, 0.0, 30.0, 0.0];
        let mut batched = BrushStrokeEngine::new(100.0);
        batched.begin(moves[0], moves[1]);
        let n_batch = batched.update_batch(&moves);
        let mut stepped = BrushStrokeEngine::new(100.0);
        stepped.begin(moves[0], moves[1]);
        let mut all: Vec<f64> = Vec::new();
        let mut i = 0;
        while i + 1 < moves.len() {
            stepped.update(moves[i], moves[i + 1]);
            all.extend_from_slice(stepped.dab_slice());
            i += 2;
        }
        assert_eq!(n_batch * 2, all.len());
        assert_eq!(batched.dab_slice(), all.as_slice());
    }
}
