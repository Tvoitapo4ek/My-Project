//! Magical network step — pure data, unit-tested.
//!
//! Magic is not one number: every node holds an **amount** of one **kind**
//! with a **stability** (0..1) and a **distortion** (0..1). Each tick:
//!
//! 1. sources (collectors) add their inflow;
//! 2. links move magic from fuller to emptier nodes (`pressure` =
//!    fill ratio), at most their **throughput**, losing a share; a node
//!    refuses another kind, a kind its runes filter out, or magic more
//!    distorted than it tolerates (**compatibility**); links add their
//!    instability (resonance interference);
//! 3. stabilizers lower distortion and raise stability (if powered);
//! 4. consumers take what they need.

use crate::content::AetherKind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality {
    pub kind: AetherKind,
    pub stability: f32,
    pub distortion: f32,
}

impl Quality {
    /// What a kind looks like straight from its source.
    pub fn raw(kind: AetherKind) -> Self {
        let (stability, distortion) = match kind {
            AetherKind::Calm => (0.9, 0.05),
            AetherKind::Agitated => (0.35, 0.6),
            AetherKind::Condensed => (0.5, 0.45),
            AetherKind::Ancient => (1.0, 0.0),
            AetherKind::Unique => (0.2, 0.85),
        };
        Self { kind, stability, distortion }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MNode {
    pub amount: f32,
    pub capacity: f32,
    /// Kind and quality of what is held (meaningful when amount > 0).
    pub quality: Quality,
    /// Kinds the node accepts (rune filter / device design).
    pub accepts: [bool; 5],
    /// Refuses magic more distorted than this.
    pub max_distortion: f32,
    /// Collector inflow per second.
    pub inflow: Option<(Quality, f32)>,
    /// Consumer: wanted per second.
    pub consume: f32,
    /// Max outflow per second (rune limit); INFINITY = none.
    pub out_limit: f32,
    /// Stabilizer strength per second (already scaled by its power).
    pub stabilize: f32,
    /// Direction runes.
    pub send: bool,
    pub receive: bool,
}

impl Default for MNode {
    fn default() -> Self {
        Self {
            amount: 0.0,
            capacity: 10.0,
            quality: Quality::raw(AetherKind::Calm),
            accepts: [true; 5],
            max_distortion: 1.0,
            inflow: None,
            consume: 0.0,
            out_limit: f32::INFINITY,
            stabilize: 0.0,
            send: true,
            receive: true,
        }
    }
}

pub fn kind_index(k: AetherKind) -> usize {
    AetherKind::ALL.iter().position(|x| *x == k).unwrap_or(0)
}

impl MNode {
    fn pressure(&self) -> f32 {
        if self.capacity <= 0.0 { 1.0 } else { self.amount / self.capacity }
    }
    /// Why `q` cannot enter (None = it can).
    pub fn refuses(&self, q: &Quality) -> Option<Refusal> {
        if !self.receive {
            return Some(Refusal::Direction);
        }
        if !self.accepts[kind_index(q.kind)] {
            return Some(Refusal::Kind);
        }
        if self.amount > 1e-4 && self.quality.kind != q.kind {
            return Some(Refusal::Mixing);
        }
        if q.distortion > self.max_distortion + 1e-4 {
            return Some(Refusal::Distortion);
        }
        None
    }
    fn add(&mut self, amount: f32, q: Quality) {
        if amount <= 0.0 {
            return;
        }
        let total = self.amount + amount;
        if self.amount <= 1e-4 {
            self.quality = q;
        } else {
            let w = amount / total;
            self.quality.stability = self.quality.stability * (1.0 - w) + q.stability * w;
            self.quality.distortion = self.quality.distortion * (1.0 - w) + q.distortion * w;
        }
        self.amount = total.min(self.capacity);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Kind,
    Mixing,
    Distortion,
    Direction,
}

impl Refusal {
    pub fn label(self) -> &'static str {
        match self {
            Refusal::Kind => "тип магии не подходит (фильтр/устройство)",
            Refusal::Mixing => "внутри другой тип магии — смешивать нельзя",
            Refusal::Distortion => "слишком искажённая магия — нужен стабилизатор",
            Refusal::Direction => "руна направления запрещает приём",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MEdge {
    pub a: usize,
    pub b: usize,
    /// Per second.
    pub throughput: f32,
    /// Share lost on the way (0..1).
    pub loss: f32,
    /// Only a → b (rune channels, resonance).
    pub one_way: bool,
    /// Added to distortion / removed from stability of what passes.
    pub noise: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepResult {
    /// Taken by consumers this step (per node).
    pub consumed: Vec<f32>,
    /// Delivered amount per edge this step.
    pub flow: Vec<f32>,
    /// Why an edge carried nothing although it could.
    pub refused: Vec<Option<Refusal>>,
    /// Lost on the way.
    pub lost: f32,
}

pub fn step(nodes: &mut [MNode], edges: &[MEdge], dt: f32) -> StepResult {
    let mut r = StepResult { consumed: vec![0.0; nodes.len()], flow: vec![0.0; edges.len()], refused: vec![None; edges.len()], lost: 0.0 };
    // 1. Inflow.
    for n in nodes.iter_mut() {
        if let Some((q, rate)) = n.inflow
            && n.refuses(&q).is_none()
        {
            n.add(rate * dt, q);
        }
    }
    // 2. Links (one pass, in order).
    let mut sent = vec![0.0f32; nodes.len()];
    for (i, e) in edges.iter().enumerate() {
        let (pa, pb) = (nodes[e.a].pressure(), nodes[e.b].pressure());
        let (s, d) = if pa >= pb { (e.a, e.b) } else if e.one_way { continue } else { (e.b, e.a) };
        if (nodes[s].pressure() - nodes[d].pressure()) < 1e-4 || nodes[s].amount <= 1e-4 || !nodes[s].send {
            continue;
        }
        let mut q = nodes[s].quality;
        q.distortion = (q.distortion + e.noise).min(1.0);
        q.stability = (q.stability - e.noise).max(0.0);
        if let Some(why) = nodes[d].refuses(&q) {
            r.refused[i] = Some(why);
            continue;
        }
        let (src, dst) = (&nodes[s], &nodes[d]);
        let equalize = (src.pressure() - dst.pressure()) * 0.5 * src.capacity.min(dst.capacity);
        let room = (dst.capacity - dst.amount).max(0.0);
        let limit_left = (src.out_limit * dt - sent[s]).max(0.0);
        let f = equalize.min(e.throughput * dt).min(src.amount).min(limit_left).min(room / (1.0 - e.loss).max(1e-3));
        if f <= 1e-6 {
            continue;
        }
        let delivered = f * (1.0 - e.loss);
        nodes[s].amount -= f;
        sent[s] += f;
        nodes[d].add(delivered, q);
        r.flow[i] = delivered;
        r.lost += f - delivered;
    }
    // 3. Stabilizers.
    for n in nodes.iter_mut() {
        if n.stabilize > 0.0 && n.amount > 1e-4 {
            n.quality.distortion = (n.quality.distortion - n.stabilize * dt).max(0.0);
            n.quality.stability = (n.quality.stability + n.stabilize * dt).min(1.0);
        }
    }
    // 4. Consumers.
    for (i, n) in nodes.iter_mut().enumerate() {
        if n.consume > 0.0 {
            let take = (n.consume * dt).min(n.amount);
            n.amount -= take;
            r.consumed[i] = take;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(a: usize, b: usize, throughput: f32) -> MEdge {
        MEdge { a, b, throughput, loss: 0.0, one_way: false, noise: 0.0 }
    }

    fn run(nodes: &mut [MNode], edges: &[MEdge], secs: f32) -> StepResult {
        let mut last = StepResult::default();
        for _ in 0..(secs * 60.0) as usize {
            last = step(nodes, edges, 1.0 / 60.0);
        }
        last
    }

    #[test]
    fn collector_fills_storage_and_consumer_uses_it() {
        let calm = Quality::raw(AetherKind::Calm);
        let mut n = [
            MNode { inflow: Some((calm, 2.0)), capacity: 4.0, ..Default::default() },
            MNode { capacity: 50.0, ..Default::default() },
            MNode { capacity: 2.0, consume: 0.5, ..Default::default() },
        ];
        let e = [link(0, 1, 5.0), link(1, 2, 5.0)];
        let r = run(&mut n, &e, 10.0);
        assert!(n[1].amount > 5.0, "{:?}", n[1]);
        assert!(r.consumed[2] > 0.0);
    }

    #[test]
    fn throughput_and_loss_limit_transfer() {
        let mut n = [MNode { amount: 100.0, capacity: 100.0, ..Default::default() }, MNode { capacity: 100.0, ..Default::default() }];
        run(&mut n, &[MEdge { loss: 0.2, ..link(0, 1, 1.0) }], 5.0);
        assert!((n[0].amount - 95.0).abs() < 0.2, "{:?}", n[0]);
        assert!((n[1].amount - 4.0).abs() < 0.2, "{:?}", n[1]);
    }

    #[test]
    fn kinds_do_not_mix_and_filters_refuse() {
        let mut n = [
            MNode { amount: 5.0, quality: Quality::raw(AetherKind::Agitated), ..Default::default() },
            MNode { amount: 1.0, quality: Quality::raw(AetherKind::Calm), ..Default::default() },
        ];
        let r = step(&mut n, &[link(0, 1, 5.0)], 0.1);
        assert_eq!(r.refused[0], Some(Refusal::Mixing));
        let mut n = [
            MNode { amount: 5.0, quality: Quality::raw(AetherKind::Agitated), ..Default::default() },
            MNode { accepts: [true, false, true, true, true], ..Default::default() },
        ];
        let r = step(&mut n, &[link(0, 1, 5.0)], 0.1);
        assert_eq!(r.refused[0], Some(Refusal::Kind));
    }

    #[test]
    fn distorted_magic_needs_a_stabilizer() {
        let agitated = Quality::raw(AetherKind::Agitated);
        // Engine tolerates distortion 0.3 at most.
        let mut n = [
            MNode { amount: 8.0, quality: agitated, ..Default::default() },
            MNode { capacity: 2.0, consume: 1.0, max_distortion: 0.3, ..Default::default() },
        ];
        let r = step(&mut n, &[link(0, 1, 5.0)], 0.1);
        assert_eq!(r.refused[0], Some(Refusal::Distortion));
        // Through a stabilizer it arrives calmed down.
        let mut n = [
            MNode { amount: 8.0, quality: agitated, ..Default::default() },
            MNode { capacity: 4.0, stabilize: 0.5, ..Default::default() },
            MNode { capacity: 2.0, consume: 1.0, max_distortion: 0.3, ..Default::default() },
        ];
        let r = run(&mut n, &[link(0, 1, 5.0), link(1, 2, 5.0)], 4.0);
        assert!(r.consumed[2] > 0.0 && n[2].quality.distortion <= 0.3, "{:?}", n);
    }

    #[test]
    fn one_way_channel_and_runes() {
        let mut n = [MNode::default(), MNode { amount: 5.0, ..Default::default() }];
        let e = [MEdge { one_way: true, ..link(0, 1, 5.0) }];
        run(&mut n, &e, 1.0);
        assert_eq!(n[0].amount, 0.0, "nothing flows backwards");
        // Limit rune: at most 0.5 per second out.
        let mut n = [MNode { amount: 10.0, out_limit: 0.5, ..Default::default() }, MNode::default()];
        run(&mut n, &[link(0, 1, 5.0)], 2.0);
        assert!((n[1].amount - 1.0).abs() < 0.05, "{:?}", n[1]);
        // Direction rune: the node does not send.
        let mut n = [MNode { amount: 10.0, send: false, ..Default::default() }, MNode::default()];
        run(&mut n, &[link(0, 1, 5.0)], 1.0);
        assert_eq!(n[1].amount, 0.0);
    }

    #[test]
    fn noise_degrades_quality() {
        let mut n = [MNode { amount: 10.0, ..Default::default() }, MNode::default()];
        run(&mut n, &[MEdge { noise: 0.1, ..link(0, 1, 5.0) }], 1.0);
        assert!(n[1].quality.distortion > n[0].quality.distortion);
    }
}
