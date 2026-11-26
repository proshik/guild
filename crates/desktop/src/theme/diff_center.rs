//! Dedicated diff-center semantics, section 6.10. Kept out of widget literals.
use super::Theme;
#[derive(Clone, Copy)]
pub(crate) struct Colors {
    pub added: u32,
    pub removed: u32,
    pub modified: u32,
    pub empty: u32,
    pub blame: [u32; 3],
    pub metadata: u32,
    pub number: u32,
}
pub(crate) fn of(theme: Theme) -> Colors {
    if theme.kind.is_light() {
        Colors {
            added: 0xdcecdf,
            removed: 0xd5d8da,
            modified: 0xdce8f3,
            empty: 0xebedf0,
            blame: [0xaec9ef, 0xc4d7f0, 0xdbe5f3],
            metadata: 0x425675,
            number: 0x68788b,
        }
    } else {
        Colors {
            added: 0x264536,
            removed: 0x454849,
            modified: 0x243746,
            empty: 0x17191d,
            blame: [0x355b95, 0x304d79, 0x2a3c59],
            metadata: 0xa7b5ce,
            number: 0x717884,
        }
    }
}
