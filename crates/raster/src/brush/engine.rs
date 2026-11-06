// SPDX-License-Identifier: AGPL-3.0-or-later
// Brush dab producer: the stroke spacing/carry state machine behind the brush
// tip mask. It defines the dab-interpolation and spacing rules
// (`BrushStrokeEngine::update` + `brush_dab_spacing`) in f64 throughout, so a
// given input emits the same dab sequence on every run.
// Dabs are accumulated into an internal Vec<f64> (x0,y0,x1,y1,...) and cleared
// on each update() call. update()/update_batch() return the count of x,y
// pairs produced; the raw buffer is readable in-crate via `dab_slice`.

/// spacing = max(1, round(size * 0.10)): 10% of the brush size, at least 1.
pub fn brush_dab_spacing(size: f64) -> f64 {
    (size * 0.10).round().max(1.0)
}

pub struct BrushStrokeEngine {
    last_x: f64,
    last_y: f64,
    spacing: f64,
    carry: f64,
    started: bool,
    dabs: Vec<f64>, // x0,y0,x1,y1,... cleared per update
}

impl BrushStrokeEngine {
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

    /// Update spacing when brush size changes mid-stroke. The carry persists,
    /// so the stroke keeps its position in the spacing sequence.
    pub fn set_size(&mut self, size: f64) {
        self.spacing = brush_dab_spacing(size);
    }

    /// Begin/reposition the stroke anchor. Emits no dab: the caller emits the
    /// first stamp.
    pub fn begin(&mut self, x: f64, y: f64) {
        self.last_x = x;
        self.last_y = y;
        self.started = true;
        self.dabs.clear();
    }

    /// Feed one pointer move. Returns the number of dabs produced; read them
    /// via dab_slice() (2 * count f64s: x,y pairs).
    pub fn update(&mut self, x: f64, y: f64) -> usize {
        self.dabs.clear();
        self.append_move(x, y)
    }

    /// Batch feed (per-frame shape): moves is flat [x0,y0,x1,y1,...]. Clears
    /// the dab buffer once, appends across all moves; returns total dabs.
    /// Consumes one buffer per frame.
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

    pub fn carry(&self) -> f64 {
        self.carry
    }

    pub fn spacing(&self) -> f64 {
        self.spacing
    }

    /// Dabs produced by the last `update`/`update_batch`, as flat x,y pairs.
    ///
    /// The native input chain feeds these to `PixelStore::commit_pixels`; the
    /// buffer is cleared on the next update.
    pub fn dabs(&self) -> &[f64] {
        &self.dabs
    }

    /// Test-only direct access to the raw dab buffer.
    #[cfg(test)]
    pub(crate) fn dab_slice(&self) -> &[f64] {
        &self.dabs
    }

    /// Test-only: seed carry to reproduce reference cases that inject
    /// values unreachable via natural chaining (carry > spacing never occurs
    /// in live sessions).
    #[cfg(test)]
    pub(crate) fn seed_carry(&mut self, c: f64) {
        self.carry = c;
    }
}
