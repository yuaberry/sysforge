//! Subsistema de boot: entradas UEFI e ESP (EFI System Partition).

pub mod efi;
pub mod esp;

pub use efi::{read_efi_state, EfiBootEntry, EfiBootState};
pub use esp::{read_esp, EspInfo};
