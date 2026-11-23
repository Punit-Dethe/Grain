//! Read the EWMH icon for the X11 window active at the start of dictation.

use super::{to_icon, PILL_ICON_PX};

#[cfg(target_os = "linux")]
pub(super) fn foreground_icon() -> Option<Vec<u8>> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    // Bound each read to 1 MiB. A truncated response can still include a complete icon.
    const MAX_PROPERTY_WORDS: u32 = 256 * 1024;

    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let active_atom = conn.intern_atom(true, b"_NET_ACTIVE_WINDOW").ok()?;
    let icon_atom = conn.intern_atom(true, b"_NET_WM_ICON").ok()?;
    let (active_atom, icon_atom) = (active_atom.reply().ok()?.atom, icon_atom.reply().ok()?.atom);
    if active_atom == 0 || icon_atom == 0 {
        return None;
    }

    let active_window = || {
        let reply = conn
            .get_property(false, root, active_atom, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let window = reply.value32()?.next()?;
        (window != 0).then_some(window)
    };
    let window = active_window()?;
    let reply = conn
        .get_property(
            false,
            window,
            icon_atom,
            AtomEnum::CARDINAL,
            0,
            MAX_PROPERTY_WORDS,
        )
        .ok()?
        .reply()
        .ok()?;
    let words: Vec<_> = reply.value32()?.collect();
    let icon = decode(&words)?;
    (active_window() == Some(window)).then_some(icon)
}

fn decode(mut words: &[u32]) -> Option<Vec<u8>> {
    let mut best = None;
    let mut best_rank = (true, usize::MAX);
    while let [width, height, pixels @ ..] = words {
        let (width, height) = (*width as usize, *height as usize);
        let Some((icon, rest)) = width
            .checked_mul(height)
            .and_then(|n| pixels.split_at_checked(n))
        else {
            break;
        };
        words = rest;
        let short_side = width.min(height);
        if short_side == 0 {
            continue;
        }
        let rank = (short_side < PILL_ICON_PX, short_side.abs_diff(PILL_ICON_PX));
        if rank < best_rank {
            best = Some((width, height, icon));
            best_rank = rank;
        }
    }

    let (width, height, argb) = best?;
    let mut rgba = Vec::with_capacity(argb.len() * 4);
    for word in argb {
        let [a, r, g, b] = word.to_be_bytes();
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    to_icon(&rgba, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_best_complete_icon_and_decodes_argb() {
        let size = PILL_ICON_PX as u32;
        let mut words = vec![1, 1, 0xffff_0000, size, size];
        words.extend(std::iter::repeat_n(
            0x8000_8040,
            PILL_ICON_PX * PILL_ICON_PX,
        ));
        words.extend([u32::MAX, u32::MAX, 0]);
        assert_eq!(decode(&words).unwrap()[..4], [0, 64, 32, 128]);
        assert!(decode(&[2, 2, 0xff00_0000]).is_none());
    }
}
