//! Manettes (XInput sous Windows, evdev sous Linux) via `gilrs`.
//!
//! Le thread convertit boutons et stick gauche en actions de navigation, avec
//! répétition automatique quand une direction est maintenue. Il publie aussi la
//! liste des manettes (nom, marque, batterie, dernière utilisée). Les touches ne
//! servent qu'à naviguer dans Halcyon : rien n'est intercepté pendant une partie.

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs, PowerInfo};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadAction {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    Search,
    Favorite,
    Menu,
    /// Menu contextuel de l'élément sélectionné (bureau).
    Context,
    /// Onglet ou vue précédente / suivante (bureau).
    Prev,
    Next,
    /// Bibliothèque : cocher le jeu sélectionné ; mode console : changer de vue (étagère).
    Toggle,
    /// Mode console : inspecter le boîtier choisi en 3D (1.0.0-alpha.8).
    Inspect,
}

impl PadAction {
    pub fn as_input(self) -> Option<&'static str> {
        Some(match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Accept => "accept",
            Self::Back => "back",
            Self::Search => "search",
            Self::Favorite => "fav",
            Self::Inspect => "inspect",
            Self::Menu | Self::Context | Self::Prev | Self::Next | Self::Toggle => return None,
        })
    }
}

/// Actions configurables (les directions restent sur la croix et le stick gauche).
pub const CONFIGURABLE: [(&str, &str, PadAction, Button); 10] = [
    ("accept", tr_noop!("Valider / lancer"), PadAction::Accept, Button::South),
    ("back", tr_noop!("Retour"), PadAction::Back, Button::East),
    ("search", tr_noop!("Recherche"), PadAction::Search, Button::North),
    ("fav", tr_noop!("Favori"), PadAction::Favorite, Button::West),
    ("menu", tr_noop!("Entrer / quitter Big Picture"), PadAction::Menu, Button::Start),
    ("context", tr_noop!("Menu contextuel"), PadAction::Context, Button::Select),
    ("prev", tr_noop!("Onglet ou vue précédente"), PadAction::Prev, Button::LeftTrigger),
    ("next", tr_noop!("Onglet ou vue suivante"), PadAction::Next, Button::RightTrigger),
    ("toggle", tr_noop!("Cocher un jeu (bibliothèque) · étagère (console)"), PadAction::Toggle, Button::RightThumb),
    ("inspect", tr_noop!("Inspecter le boîtier en 3D (console)"), PadAction::Inspect, Button::LeftThumb),
];

/// Boutons proposés à l'association (noms stables, enregistrés dans les paramètres).
const BUTTONS: [(&str, Button); 12] = [
    ("South", Button::South),
    ("East", Button::East),
    ("North", Button::North),
    ("West", Button::West),
    ("LeftTrigger", Button::LeftTrigger),
    ("RightTrigger", Button::RightTrigger),
    ("LeftTrigger2", Button::LeftTrigger2),
    ("RightTrigger2", Button::RightTrigger2),
    ("Select", Button::Select),
    ("Start", Button::Start),
    ("LeftThumb", Button::LeftThumb),
    ("RightThumb", Button::RightThumb),
];

pub fn button_from_name(name: &str) -> Option<Button> {
    BUTTONS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

pub fn brand_for(vendor: Option<u16>, name: &str) -> &'static str {
    match vendor {
        Some(0x045e) => "Xbox",
        Some(0x054c) => "PlayStation",
        Some(0x057e) => "Nintendo",
        Some(0x28de) => "Valve",
        Some(0x0f0d) => "Hori",
        Some(0x0e6f) => "PDP",
        Some(0x046d) => "Logitech",
        Some(0x2dc8) => "8BitDo",
        Some(0x1532) => "Razer",
        _ => {
            let n = name.to_lowercase();
            if n.contains("xbox") || n.contains("xinput") {
                "Xbox"
            } else if n.contains("dualsense") || n.contains("dualshock") || n.contains("playstation") {
                "PlayStation"
            } else if n.contains("nintendo") || n.contains("switch") || n.contains("joy-con") {
                "Nintendo"
            } else {
                tr_noop!("Générique")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PadInfo {
    pub name: String,
    pub brand: String,
    pub power: String,
    pub last_used: bool,
}

pub enum PadEvent {
    Action(PadAction),
    Devices(Vec<PadInfo>),
}

/// Réglages partagés avec le thread des manettes.
struct Shared {
    mapping: Mutex<HashMap<Button, PadAction>>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared { mapping: Mutex::new(default_mapping()) })
}

fn default_mapping() -> HashMap<Button, PadAction> {
    let mut m: HashMap<Button, PadAction> = CONFIGURABLE.iter().map(|(_, _, a, b)| (*b, *a)).collect();
    m.insert(Button::Mode, PadAction::Menu);
    m
}

/// Applique une association action → nom de bouton (absente : disposition standard).
pub fn set_mapping(overrides: &std::collections::BTreeMap<String, String>) {
    let mut m: HashMap<Button, PadAction> = HashMap::new();
    for (key, _, action, default) in CONFIGURABLE {
        let b = overrides.get(key).and_then(|n| button_from_name(n)).unwrap_or(default);
        m.insert(b, action);
    }
    // Le bouton central (Guide/PS/Home) ouvre toujours Big Picture.
    m.entry(Button::Mode).or_insert(PadAction::Menu);
    *shared().mapping.lock().unwrap_or_else(|e| e.into_inner()) = m;
}

const FIRST_REPEAT: Duration = Duration::from_millis(380);
const REPEAT: Duration = Duration::from_millis(110);
const STICK_ON: f32 = 0.6;
const STICK_OFF: f32 = 0.35;
const POWER_POLL: Duration = Duration::from_secs(30);
/// Attente maximale du thread au repos (ms) : la batterie est relue toutes les 30 s.
const IDLE_WAIT_MS: u64 = 1000;

fn power_label(p: PowerInfo) -> String {
    match p {
        PowerInfo::Wired => tr!("Filaire"),
        PowerInfo::Discharging(l) => tr!("Batterie {l} %", l = l),
        PowerInfo::Charging(l) => tr!("En charge ({l} %)", l = l),
        PowerInfo::Charged => tr!("Batterie pleine"),
        PowerInfo::Unknown => tr!("Alimentation inconnue"),
    }
}

fn devices(gilrs: &Gilrs, last: Option<GamepadId>) -> Vec<PadInfo> {
    gilrs
        .gamepads()
        .map(|(id, g)| PadInfo {
            name: g.name().to_string(),
            brand: brand_for(g.vendor_id(), g.name()).to_string(),
            power: power_label(g.power_info()),
            last_used: Some(id) == last,
        })
        .collect()
}

pub fn spawn(send: impl Fn(PadEvent) + Send + 'static) {
    std::thread::Builder::new()
        .name("halcyon-gamepad".into())
        .spawn(move || {
            let mut gilrs = match Gilrs::new() {
                Ok(g) => g,
                Err(e) => {
                    log::warn!("manettes indisponibles : {e}");
                    return;
                }
            };
            let mut last: Option<GamepadId> = None;
            let mut published = devices(&gilrs, last);
            send(PadEvent::Devices(published.clone()));
            let mut last_poll = Instant::now();
            let (mut stick_x, mut stick_y) = (0.0f32, 0.0f32);
            let mut held: Option<(PadAction, Instant, bool)> = None;
            loop {
                let mut changed = false;
                // Au repos (aucune direction maintenue, stick au centre) : réveil rare. Sinon 40 ms
                // pour la répétition automatique et le stick.
                let busy = held.is_some() || stick_x.abs() > STICK_OFF || stick_y.abs() > STICK_OFF;
                let wait = Duration::from_millis(if busy { 40 } else { IDLE_WAIT_MS });
                while let Some(ev) = gilrs.next_event_blocking(Some(wait)) {
                    if last != Some(ev.id) && !matches!(ev.event, EventType::Disconnected) {
                        last = Some(ev.id);
                        changed = true;
                    }
                    match ev.event {
                        EventType::Connected | EventType::Disconnected => changed = true,
                        EventType::ButtonPressed(b, _) => {
                            let action = match b {
                                Button::DPadUp => Some(PadAction::Up),
                                Button::DPadDown => Some(PadAction::Down),
                                Button::DPadLeft => Some(PadAction::Left),
                                Button::DPadRight => Some(PadAction::Right),
                                other => shared().mapping.lock().unwrap_or_else(|e| e.into_inner()).get(&other).copied(),
                            };
                            if let Some(a) = action {
                                send(PadEvent::Action(a));
                                if matches!(a, PadAction::Up | PadAction::Down | PadAction::Left | PadAction::Right) {
                                    held = Some((a, Instant::now(), false));
                                }
                            }
                        }
                        EventType::ButtonReleased(b, _) => {
                            if matches!(b, Button::DPadUp | Button::DPadDown | Button::DPadLeft | Button::DPadRight) {
                                held = None;
                            }
                        }
                        EventType::AxisChanged(Axis::LeftStickX, v, _) => {
                            stick_x = v;
                            break; // évaluer la direction tout de suite
                        }
                        EventType::AxisChanged(Axis::LeftStickY, v, _) => {
                            stick_y = v;
                            break;
                        }
                        _ => {}
                    }
                }
                if changed || last_poll.elapsed() >= POWER_POLL {
                    last_poll = Instant::now();
                    let now = devices(&gilrs, last);
                    if now != published {
                        published = now.clone();
                        send(PadEvent::Devices(now));
                    }
                }
                // Stick gauche → direction (avec hystérésis).
                let stick_dir = if stick_x.abs() > stick_y.abs() {
                    if stick_x > STICK_ON { Some(PadAction::Right) } else if stick_x < -STICK_ON { Some(PadAction::Left) } else { None }
                } else if stick_y > STICK_ON {
                    Some(PadAction::Up)
                } else if stick_y < -STICK_ON {
                    Some(PadAction::Down)
                } else {
                    None
                };
                let stick_idle = stick_x.abs() < STICK_OFF && stick_y.abs() < STICK_OFF;
                match (&mut held, stick_dir) {
                    (None, Some(d)) => {
                        send(PadEvent::Action(d));
                        held = Some((d, Instant::now(), true));
                    }
                    (Some((_, _, true)), _) if stick_idle => held = None,
                    _ => {}
                }
                // Répétition automatique.
                if let Some((action, since, _)) = &mut held {
                    if since.elapsed() >= FIRST_REPEAT {
                        send(PadEvent::Action(*action));
                        *since = Instant::now() - (FIRST_REPEAT - REPEAT);
                    }
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brands() {
        assert_eq!(brand_for(Some(0x054c), "Wireless Controller"), "PlayStation");
        assert_eq!(brand_for(None, "Xbox Series X Controller"), "Xbox");
        assert_eq!(brand_for(Some(0x1234), "Gamepad"), "Générique");
    }

    #[test]
    fn mapping_overrides() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("accept".to_string(), "East".to_string());
        o.insert("back".to_string(), "South".to_string());
        set_mapping(&o);
        let m = shared().mapping.lock().unwrap().clone();
        assert_eq!(m.get(&Button::East), Some(&PadAction::Accept));
        assert_eq!(m.get(&Button::South), Some(&PadAction::Back));
        assert_eq!(m.get(&Button::Mode), Some(&PadAction::Menu));
        // Bouton dédié à l'étagère / à la sélection : distinct de tous les autres par défaut.
        assert_eq!(m.get(&Button::RightThumb), Some(&PadAction::Toggle));
        let defaults: std::collections::HashSet<Button> = CONFIGURABLE.iter().map(|c| c.3).collect();
        assert_eq!(defaults.len(), CONFIGURABLE.len(), "aucun bouton partagé par défaut");
        set_mapping(&Default::default());
    }
}
