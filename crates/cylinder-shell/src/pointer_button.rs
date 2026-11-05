//! Focus/position must precede each button; evdev codes pass through unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEvent {
    Motion([f32; 2]),
    Button {code: u32, pressed: bool},
}

pub fn dispatch_button<E>(position: [f32; 2], code: u32, pressed: bool,
    mut send: impl FnMut(PointerEvent) -> Result<(), E>) -> Result<(), E> {
    send(PointerEvent::Motion(position))?;
    send(PointerEvent::Button {code, pressed})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_and_right_press_release_are_not_swapped_filtered_or_consumed() {
        for code in [0x110, 0x111] {
            let mut sent = Vec::new();
            for pressed in [true, false] {
                dispatch_button([123.0, 45.0], code, pressed, |event| {
                    sent.push(event); Ok::<_, ()>(())
                }).unwrap();
            }
            assert_eq!(sent, vec![PointerEvent::Motion([123.0, 45.0]),
                PointerEvent::Button {code, pressed: true}, PointerEvent::Motion([123.0, 45.0]),
                PointerEvent::Button {code, pressed: false}]);
        }
    }

    #[test]
    fn failed_pointer_entry_is_reported_not_silently_discarded() {
        let mut calls = 0;
        let result = dispatch_button([0.0; 2], 0x110, true, |_| {calls += 1; Err("disconnected")});
        assert_eq!(result, Err("disconnected"));
        assert_eq!(calls, 1);
    }
}
