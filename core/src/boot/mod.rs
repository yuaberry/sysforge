//! Subsistema de boot: entradas UEFI, ESP e snapshots de estado.

pub mod efi;
pub mod esp;
pub mod snapshot;

pub use efi::{read_efi_state, EfiBootEntry, EfiBootState};
pub use esp::{read_esp, EspInfo};
pub use snapshot::BootSnapshot;
