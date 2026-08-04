//! Аппаратные сведения о машине: определяются один раз при старте,
//! на их основе подбираются параметры ASR (потоки ORT).

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub cpu_brand: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub total_memory_mb: u64,
}

static HARDWARE: OnceLock<HardwareInfo> = OnceLock::new();

/// Определить и закэшировать аппаратные сведения (один раз на процесс).
pub fn detect() -> &'static HardwareInfo {
    HARDWARE.get_or_init(|| {
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu_list(sysinfo::CpuRefreshKind::everything());
        sys.refresh_memory();
        HardwareInfo {
            cpu_brand: sys
                .cpus()
                .first()
                .map(|c| c.brand().to_string())
                .unwrap_or_default(),
            physical_cores: sysinfo::System::physical_core_count(),
            logical_cores: sys.cpus().len().max(1),
            total_memory_mb: sys.total_memory() / (1024 * 1024),
        }
    })
}

/// Потоки ORT для ASR-энкодера (intra-op).
///
/// GigaAM-V3 int8 — bandwidth-bound: на одном и том же ролике 8 потоков
/// измеренно в 1.8 раза медленнее 4 (RTF 0.089 → 0.164), поэтому потолок
/// 4. На слабых машинах урезаем до числа физических ядер (логические ядра
/// с HT не помогают bandwidth-bound инференсу).
pub fn asr_threads() -> usize {
    let hw = detect();
    let cores = hw.physical_cores.unwrap_or(hw.logical_cores);
    cores.min(4).max(1)
}
