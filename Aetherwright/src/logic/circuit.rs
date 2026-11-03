//! Logic circuit evaluation — pure data, unit-tested.
//!
//! Signals are numbers 0..1 (above 0.5 = "on"). Every tick each element
//! computes its output from the *previous* tick's outputs of the elements
//! wired into it. One tick of delay per element makes feedback (latches,
//! oscillators) well-defined and deterministic, like real relays.

use serde::{Deserialize, Serialize};

pub const ON: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SensorKind {
    /// |rpm| / 256 of the kinetic block next to it.
    Rotation,
    /// Fill of the energy storage of the nearest device's network.
    Energy,
    /// Fill of the adjacent magical device.
    Aether,
    /// Distortion of the adjacent magical device.
    Distortion,
    /// 1 when the player or a creature is within a few metres.
    Presence,
    /// Strength of an anomaly at the sensor (|gravity − 1|, capped at 1).
    Anomaly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Element {
    Sensor(SensorKind),
    Button,
    Lever,
    /// Pulse every `TIMER_PERIODS[param]` seconds.
    Timer,
    /// Counts rising edges; on when the count reaches `COUNTER_GOALS[param]`.
    Counter,
    And,
    Or,
    Not,
    Xor,
    /// `input > threshold` (mode 0) or `input < threshold` (mode 1);
    /// threshold = `(param + 1) / 10`.
    Comparator,
    /// Commands a machine: mode 0 — on while the input is on, mode 1 — on
    /// while the input is off, mode 2 — toggles on each rising edge.
    Controller,
    /// Wireless sender / receiver on channel `param`.
    Transmitter,
    Receiver,
    /// Shows the input (telemetry lamp).
    Lamp,
}

pub const TIMER_PERIODS: [f32; 4] = [1.0, 2.0, 5.0, 10.0];
pub const COUNTER_GOALS: [u32; 4] = [2, 4, 8, 16];

impl Element {
    pub fn params(self) -> u8 {
        match self {
            Element::Timer | Element::Counter => 4,
            Element::Comparator => 9,
            Element::Transmitter | Element::Receiver => 8,
            _ => 1,
        }
    }
    pub fn modes(self) -> u8 {
        match self {
            Element::Comparator => 2,
            Element::Controller => 3,
            _ => 1,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Element::Sensor(SensorKind::Rotation) => "Датчик оборотов",
            Element::Sensor(SensorKind::Energy) => "Датчик заряда",
            Element::Sensor(SensorKind::Aether) => "Датчик эфира",
            Element::Sensor(SensorKind::Distortion) => "Датчик искажения",
            Element::Sensor(SensorKind::Presence) => "Датчик присутствия",
            Element::Sensor(SensorKind::Anomaly) => "Датчик аномалий",
            Element::Button => "Кнопка",
            Element::Lever => "Рычаг",
            Element::Timer => "Таймер",
            Element::Counter => "Счётчик",
            Element::And => "И",
            Element::Or => "ИЛИ",
            Element::Not => "НЕ",
            Element::Xor => "Исключающее ИЛИ",
            Element::Comparator => "Компаратор",
            Element::Controller => "Контроллер",
            Element::Transmitter => "Сигнальный передатчик",
            Element::Receiver => "Сигнальный приёмник",
            Element::Lamp => "Индикатор",
        }
    }
}

/// One element with its settings and internal state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    pub element: Element,
    pub param: u8,
    pub mode: u8,
    /// Lever position / controller toggle state / counter value.
    pub state: f32,
    /// Timer phase or button time left (s).
    pub timer: f32,
    /// Last tick's input (edge detection).
    pub last_in: f32,
    /// Output of the last tick.
    pub out: f32,
}

impl Gate {
    pub fn new(element: Element) -> Self {
        Self { element, param: 0, mode: 0, state: 0.0, timer: 0.0, last_in: 0.0, out: 0.0 }
    }
}

/// Inputs from outside the circuit for one tick.
#[derive(Debug, Clone, Default)]
pub struct External {
    /// Sensor readings per element index (0..1).
    pub sensor: Vec<f32>,
    /// Buttons pressed / levers flipped this tick.
    pub pressed: Vec<bool>,
}

/// Computes new outputs. `wires[i]` = indices feeding element `i`;
/// `wireless` = (channel, value) of transmitters from the last tick are
/// derived inside.
pub fn tick(gates: &mut [Gate], wires: &[Vec<usize>], ext: &External, dt: f32) {
    let prev: Vec<f32> = gates.iter().map(|g| g.out).collect();
    // Wireless: last tick's transmitter inputs per channel.
    let mut channels = [0.0f32; 8];
    for (i, g) in gates.iter().enumerate() {
        if g.element == Element::Transmitter {
            let v = wires[i].iter().map(|j| prev[*j]).fold(0.0, f32::max);
            let c = g.param as usize % 8;
            channels[c] = channels[c].max(v);
        }
    }
    for (i, g) in gates.iter_mut().enumerate() {
        let inputs: Vec<f32> = wires[i].iter().map(|j| prev[*j]).collect();
        let any = inputs.iter().copied().fold(0.0, f32::max);
        let on = |v: f32| v > ON;
        let pressed = ext.pressed.get(i).copied().unwrap_or(false);
        let out = match g.element {
            Element::Sensor(_) => ext.sensor.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0),
            Element::Button => {
                if pressed {
                    g.timer = 1.0;
                }
                g.timer = (g.timer - dt).max(0.0);
                if g.timer > 0.0 { 1.0 } else { 0.0 }
            }
            Element::Lever => {
                if pressed {
                    g.state = 1.0 - g.state;
                }
                g.state
            }
            Element::Timer => {
                let period = TIMER_PERIODS[g.param as usize % 4];
                g.timer += dt;
                if g.timer >= period {
                    g.timer -= period;
                    1.0
                } else if g.timer < 0.25 && g.state > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Element::Counter => {
                let goal = COUNTER_GOALS[g.param as usize % 4] as f32;
                if on(any) && !on(g.last_in) {
                    g.state += 1.0;
                    if g.state > goal {
                        g.state = 1.0;
                    }
                }
                if g.state >= goal { 1.0 } else { 0.0 }
            }
            Element::And => bool_out(!inputs.is_empty() && inputs.iter().all(|v| on(*v))),
            Element::Or => bool_out(inputs.iter().any(|v| on(*v))),
            Element::Not => bool_out(!on(any)),
            Element::Xor => bool_out(inputs.iter().filter(|v| on(**v)).count() % 2 == 1),
            Element::Comparator => {
                let th = (g.param as f32 + 1.0) / 10.0;
                bool_out(if g.mode == 0 { any > th } else { any < th })
            }
            Element::Controller => match g.mode {
                0 => bool_out(on(any)),
                1 => bool_out(!on(any)),
                _ => {
                    if on(any) && !on(g.last_in) {
                        g.state = 1.0 - g.state;
                    }
                    g.state
                }
            },
            Element::Transmitter => any,
            Element::Receiver => channels[g.param as usize % 8],
            Element::Lamp => any,
        };
        // Timers keep a short "on" tail so a single tick pulse is visible.
        if g.element == Element::Timer {
            g.state = if out > 0.0 { 1.0 } else { 0.0 };
        }
        g.last_in = any;
        g.out = out;
    }
}

fn bool_out(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(g: &mut [Gate], w: &[Vec<usize>], ext: &External, ticks: usize) {
        for _ in 0..ticks {
            tick(g, w, ext, 0.1);
        }
    }

    #[test]
    fn gates_follow_their_inputs() {
        // 0: lever, 1: lever, 2: AND(0,1), 3: OR(0,1), 4: NOT(0), 5: XOR(0,1)
        let mut g = vec![Gate::new(Element::Lever), Gate::new(Element::Lever), Gate::new(Element::And), Gate::new(Element::Or), Gate::new(Element::Not), Gate::new(Element::Xor)];
        let w = vec![vec![], vec![], vec![0, 1], vec![0, 1], vec![0], vec![0, 1]];
        let mut ext = External { sensor: vec![], pressed: vec![true, false] };
        tick(&mut g, &w, &ext, 0.1);
        ext.pressed = vec![];
        run(&mut g, &w, &ext, 2);
        assert_eq!([g[2].out, g[3].out, g[4].out, g[5].out], [0.0, 1.0, 0.0, 1.0]);
        tick(&mut g, &w, &External { sensor: vec![], pressed: vec![false, true] }, 0.1);
        run(&mut g, &w, &ext, 2);
        assert_eq!([g[2].out, g[3].out, g[5].out], [1.0, 1.0, 0.0]);
    }

    #[test]
    fn sensor_comparator_controller_chain() {
        // sensor → comparator (> 0.6) → controller (mode 0)
        let mut g = vec![Gate::new(Element::Sensor(SensorKind::Energy)), Gate { param: 5, ..Gate::new(Element::Comparator) }, Gate::new(Element::Controller)];
        let w = vec![vec![], vec![0], vec![1]];
        run(&mut g, &w, &External { sensor: vec![0.4], pressed: vec![] }, 3);
        assert_eq!(g[2].out, 0.0);
        run(&mut g, &w, &External { sensor: vec![0.9], pressed: vec![] }, 3);
        assert_eq!(g[2].out, 1.0, "one tick of delay per element");
        // Inverted comparator.
        g[1].mode = 1;
        run(&mut g, &w, &External { sensor: vec![0.9], pressed: vec![] }, 3);
        assert_eq!(g[2].out, 0.0);
    }

    #[test]
    fn timer_counter_and_toggle_controller() {
        let mut g = vec![Gate::new(Element::Timer), Gate::new(Element::Counter), Gate { mode: 2, ..Gate::new(Element::Controller) }];
        let w = vec![vec![], vec![0], vec![0]];
        let ext = External::default();
        let mut pulses = 0;
        let mut toggles = 0;
        let mut last_ctrl = 0.0;
        for _ in 0..45 {
            tick(&mut g, &w, &ext, 0.1);
            if g[0].out > 0.5 {
                pulses += 1;
            }
            if g[2].out != last_ctrl {
                toggles += 1;
                last_ctrl = g[2].out;
            }
        }
        // 1 s timer for 4.5 s: 4 pulses (each a few ticks long).
        assert!((4..=12).contains(&pulses), "{pulses}");
        assert_eq!(g[1].out, 1.0, "counter reached 2 (edges 1,2,1,2)");
        assert_eq!(toggles, 4, "controller toggles on each rising edge");
    }

    #[test]
    fn wireless_and_feedback_latch() {
        // 0: button → 1: transmitter (ch 3) … 2: receiver (ch 3) → 3: lamp
        let mut g = vec![Gate::new(Element::Button), Gate { param: 3, ..Gate::new(Element::Transmitter) }, Gate { param: 3, ..Gate::new(Element::Receiver) }, Gate::new(Element::Lamp)];
        let w = vec![vec![], vec![0], vec![], vec![2]];
        tick(&mut g, &w, &External { sensor: vec![], pressed: vec![true] }, 0.1);
        run(&mut g, &w, &External::default(), 3);
        assert_eq!(g[3].out, 1.0);
        // Wrong channel hears nothing.
        g[2].param = 4;
        run(&mut g, &w, &External::default(), 3);
        assert_eq!(g[3].out, 0.0);
        // OR fed by itself holds a pulse forever (latch).
        let mut g = vec![Gate::new(Element::Button), Gate::new(Element::Or)];
        let w = vec![vec![], vec![0, 1]];
        tick(&mut g, &w, &External { sensor: vec![], pressed: vec![true] }, 0.1);
        run(&mut g, &w, &External::default(), 40);
        assert_eq!(g[1].out, 1.0);
    }
}
