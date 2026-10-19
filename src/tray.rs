//! Icône dans la zone de notification, sur un fil dédié avec sa propre boucle de messages
//! (elle doit continuer de répondre quand la fenêtre egui est masquée).

use std::{
    ptr::null_mut,
    sync::{mpsc::Sender, Arc},
    thread,
};

use eframe::egui;
use lowfreed::system::ShowSignal;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::INFINITE,
    UI::WindowsAndMessaging::{
        DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
        QS_ALLINPUT, WM_QUIT,
    },
};

pub enum TrayCommand {
    Open,
    Quit,
}

pub fn spawn(signal: ShowSignal, icon: egui::IconData, tx: Sender<TrayCommand>, ctx: egui::Context) {
    thread::Builder::new()
        .name("tray".into())
        .spawn(move || run(signal, icon, tx, ctx))
        .expect("fil tray");
}

fn run(signal: ShowSignal, icon: egui::IconData, tx: Sender<TrayCommand>, ctx: egui::Context) {
    // La fenêtre peut être masquée : on réveille egui pour qu'il traite la commande.
    let send = Arc::new(move |cmd: TrayCommand| {
        let _ = tx.send(cmd);
        ctx.request_repaint();
    });

    let open = MenuItem::new("Ouvrir Lowfreed", true, None);
    let quit = MenuItem::new("Quitter", true, None);
    let Ok(menu) = Menu::with_items(&[&open, &PredefinedMenuItem::separator(), &quit]) else { return };
    let Ok(icon) = Icon::from_rgba(icon.rgba, icon.width, icon.height) else { return };
    let Ok(_tray) = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Lowfreed — Lofree Flow 2")
        .with_icon(icon)
        .build()
    else {
        return;
    };

    let (open_id, quit_id) = (open.id().clone(), quit.id().clone());
    let s = send.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == open_id {
            s(TrayCommand::Open);
        } else if e.id == quit_id {
            s(TrayCommand::Quit);
        }
    }));
    let s = send.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if matches!(
            e,
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
        ) {
            s(TrayCommand::Open);
        }
    }));

    // Boucle de messages + événement « montre-toi » envoyé par une seconde instance.
    let handles = [signal.0];
    loop {
        // SAFETY : un handle d'événement valide pour toute la durée du processus.
        let r = unsafe { MsgWaitForMultipleObjects(1, handles.as_ptr(), 0, INFINITE, QS_ALLINPUT) };
        if r == WAIT_OBJECT_0 {
            send(TrayCommand::Open);
            continue;
        }
        // SAFETY : pompe de messages Win32 standard.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
