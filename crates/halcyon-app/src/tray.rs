//! Icône de la zone de notification : Windows (`tray-icon`), Linux (StatusNotifierItem
//! via `ksni` : KDE, GNOME avec l'extension AppIndicator, Xfce, Cinnamon…).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    Show,
    QuickLaunch,
    Mini,
    BigPicture,
    Quit,
}

const MENU: [(&str, TrayEvent); 5] = [
    (tr_noop!("Afficher Halcyon"), TrayEvent::Show),
    (tr_noop!("Lancement rapide"), TrayEvent::QuickLaunch),
    (tr_noop!("Mode compact"), TrayEvent::Mini),
    ("Big Picture", TrayEvent::BigPicture),
    (tr_noop!("Quitter Halcyon"), TrayEvent::Quit),
];

/// Pixels RVBA de l'icône (32 × 32).
fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(include_bytes!("../../../packaging/linux/halcyon.png")).ok()?;
    let img = img.resize_exact(32, 32, image::imageops::FilterType::Lanczos3).to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// Garde l'icône en vie ; la supprimer retire l'icône.
pub struct Tray {
    #[cfg(target_os = "linux")]
    _handle: ksni::blocking::Handle<LinuxTray>,
    #[cfg(windows)]
    _icon: tray_icon::TrayIcon,
    #[cfg(windows)]
    pub(crate) ids: Vec<(tray_icon::menu::MenuId, TrayEvent)>,
}

#[cfg(target_os = "linux")]
pub struct LinuxTray {
    send: Box<dyn Fn(TrayEvent) + Send>,
    icon: Vec<ksni::Icon>,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "halcyon".into()
    }
    fn title(&self) -> String {
        "Halcyon".into()
    }
    fn icon_name(&self) -> String {
        "halcyon".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Halcyon".into(), ..Default::default() }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        (self.send)(TrayEvent::Show);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let mut items = Vec::new();
        for (i, (label, ev)) in MENU.iter().enumerate() {
            if i == MENU.len() - 1 {
                items.push(ksni::MenuItem::Separator);
            }
            let ev = *ev;
            items.push(
                StandardItem { label: halcyon_i18n::t(label), activate: Box::new(move |t: &mut Self| (t.send)(ev)), ..Default::default() }
                    .into(),
            );
        }
        items
    }
}

/// Crée l'icône. `None` si le bureau n'a pas de zone de notification.
pub fn create(send: impl Fn(TrayEvent) + Send + 'static) -> Result<Tray, String> {
    #[cfg(target_os = "linux")]
    {
        use ksni::blocking::TrayMethods;
        // ARGB32 en ordre réseau.
        let icon = icon_rgba()
            .map(|(rgba, w, h)| {
                let mut data = Vec::with_capacity(rgba.len());
                for px in rgba.chunks_exact(4) {
                    data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
                }
                vec![ksni::Icon { width: w as i32, height: h as i32, data }]
            })
            .unwrap_or_default();
        let tray = LinuxTray { send: Box::new(send), icon };
        let handle = tray.spawn().map_err(|e| tr!("zone de notification indisponible : {e}", e = e))?;
        Ok(Tray { _handle: handle })
    }
    #[cfg(windows)]
    {
        use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
        let menu = Menu::new();
        let mut ids = Vec::new();
        for (i, (label, ev)) in MENU.iter().enumerate() {
            if i == MENU.len() - 1 {
                let _ = menu.append(&PredefinedMenuItem::separator());
            }
            let item = MenuItem::new(halcyon_i18n::t(label), true, None);
            ids.push((item.id().clone(), *ev));
            menu.append(&item).map_err(|e| e.to_string())?;
        }
        let mut builder = tray_icon::TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("Halcyon");
        if let Some(icon) = icon_rgba().and_then(|(rgba, w, h)| tray_icon::Icon::from_rgba(rgba, w, h).ok()) {
            builder = builder.with_icon(icon);
        }
        let icon = builder.build().map_err(|e| e.to_string())?;
        // Les évènements arrivent par des canaux globaux, relevés par `poll`.
        let _ = send;
        Ok(Tray { _icon: icon, ids })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = send;
        Err(tr!("zone de notification non prise en charge sur ce système").into())
    }
}

impl Tray {
    /// Windows : évènements en attente (clic sur l'icône, menu). Linux : remis par `send`.
    pub fn poll(&self) -> Vec<TrayEvent> {
        #[cfg(windows)]
        {
            let mut out = Vec::new();
            while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                if let Some((_, e)) = self.ids.iter().find(|(id, _)| *id == ev.id) {
                    out.push(*e);
                }
            }
            while let Ok(ev) = tray_icon::TrayIconEvent::receiver().try_recv() {
                if let tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, ..
                } = ev
                {
                    out.push(TrayEvent::Show);
                }
            }
            out
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}
