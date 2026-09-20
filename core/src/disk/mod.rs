//! Subsistema de discos: engines de inventário, identidade e saúde.

pub mod blkid;
pub mod identity;
pub mod lsblk;
pub mod smart;
pub mod udev;

pub use identity::DiskIdentity;
pub use lsblk::{list_blockdevices, probe_device, LsblkDevice};
