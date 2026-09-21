//! Instalação do Windows: checklist de pré-requisitos com sondagem REAL,
//! geração de autounattend.xml e detecção de mídia (Ventoy/USB).
//!
//! HONESTIDADE DA FASE: o YUA automatiza tudo ATÉ o reboot — mídia pronta,
//! unattend no lugar, BootNext armado. Após o reboot, quem executa é o
//! instalador do Windows (com nossas respostas). Nada é fingido.

pub mod checklist;
pub mod media;
pub mod unattend;

pub use checklist::{run_checklist, ChecklistItem, ItemStatus, WindowsChecklist};
pub use media::{list_removable_media, copy_with_progress, RemovableMedia};
pub use unattend::{generate_autounattend, UnattendConfig};
