//! Mando de juego (Xbox, PlayStation, genéricos) en un hilo propio.
//!
//! El hilo espera eventos bloqueado, así que no gasta CPU ni obliga a la
//! interfaz a redibujarse continuamente.

use crate::app::{Enviador, Msg};
use gilrs::{Axis, Button, EventType, Gilrs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accion {
    Arriba,
    Abajo,
    Izquierda,
    Derecha,
    Aceptar,
    Volver,
    Info,
    Favorito,
    PestanaAnterior,
    PestanaSiguiente,
}

pub fn iniciar(tx: Enviador) {
    std::thread::Builder::new()
        .name("mando".into())
        .spawn(move || {
            let Ok(mut g) = Gilrs::new() else { return };
            if g.gamepads().next().is_some() {
                tx.enviar(Msg::Mando(None, true));
            }
            // Estado del stick para emitir un paso al cruzar el umbral.
            let (mut x, mut y) = (0i8, 0i8);
            loop {
                let Some(ev) = g.next_event_blocking(None) else { continue };
                let accion = match ev.event {
                    EventType::Connected => {
                        tx.enviar(Msg::Mando(None, true));
                        None
                    }
                    EventType::Disconnected => {
                        tx.enviar(Msg::Mando(None, g.gamepads().count() > 1));
                        None
                    }
                    EventType::ButtonPressed(b, _) => match b {
                        Button::DPadUp => Some(Accion::Arriba),
                        Button::DPadDown => Some(Accion::Abajo),
                        Button::DPadLeft => Some(Accion::Izquierda),
                        Button::DPadRight => Some(Accion::Derecha),
                        Button::South | Button::Start => Some(Accion::Aceptar),
                        Button::East | Button::Select => Some(Accion::Volver),
                        Button::North => Some(Accion::Info),
                        Button::West => Some(Accion::Favorito),
                        Button::LeftTrigger => Some(Accion::PestanaAnterior),
                        Button::RightTrigger => Some(Accion::PestanaSiguiente),
                        _ => None,
                    },
                    EventType::AxisChanged(Axis::LeftStickX, v, _) => paso(&mut x, v, Accion::Izquierda, Accion::Derecha),
                    // En gilrs el eje Y positivo es hacia arriba.
                    EventType::AxisChanged(Axis::LeftStickY, v, _) => paso(&mut y, v, Accion::Abajo, Accion::Arriba),
                    _ => None,
                };
                if let Some(a) = accion {
                    tx.enviar(Msg::Mando(Some(a), true));
                }
            }
        })
        .ok();
}

/// Emite una acción solo al pasar de reposo a inclinado (sin repetición).
fn paso(estado: &mut i8, v: f32, negativo: Accion, positivo: Accion) -> Option<Accion> {
    let nuevo = if v > 0.6 {
        1
    } else if v < -0.6 {
        -1
    } else if v.abs() < 0.3 {
        0
    } else {
        *estado
    };
    let antes = std::mem::replace(estado, nuevo);
    match (antes, nuevo) {
        (0, 1) => Some(positivo),
        (0, -1) => Some(negativo),
        _ => None,
    }
}
