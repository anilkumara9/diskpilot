//! Drive health: pure parsers for the raw SMART payloads the shell
//! fetches (ATA SMART data structure, NVMe SMART/Health log page)
//! plus the classification that turns raw attributes into a health
//! verdict. The shell owns the IOCTLs; everything here is bytes in,
//! structured health out, bounds-checked so a malformed or truncated
//! payload can never panic — parsers simply read fewer fields.
//!
//! NVMe log-page offsets follow the NVMe specification's
//! SMART / Health Information log (Log Identifier 02h): temperature
//! at bytes 1..3, available spare at 3, percentage used at 5, power
//! cycles at 112..128, power-on hours at 128..144, unsafe shutdowns
//! at 144..160, media and data integrity errors at 160..176 (the
//! 128-bit counters are read as their low 64 bits).

use serde::{Deserialize, Serialize};

/// One ATA SMART attribute: vendor-normalized current/worst values
/// (higher is healthier) and the vendor raw value (6 bytes of the
/// 12-byte attribute record, read little-endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtaAttribute {
    pub id: u8,
    pub current: u8,
    pub worst: u8,
    pub raw: u64,
}

/// Parse the 512-byte ATA SMART data structure: a 2-byte revision
/// header, then 30 attribute records of 12 bytes each (id, 2 flag
/// bytes, current, worst, 6 raw bytes, 1 reserved). Records with
/// id 0 are empty slots and are skipped. A truncated buffer yields
/// the attributes that fit; this function never panics.
pub fn parse_ata_attributes(data: &[u8]) -> Vec<AtaAttribute> {
    let mut out = Vec::new();
    for i in 0..30 {
        let off = 2 + i * 12;
        let Some(rec) = data.get(off..off + 12) else {
            break;
        };
        let id = rec[0];
        if id == 0 {
            continue;
        }
        let mut raw_bytes = [0u8; 8];
        raw_bytes[..6].copy_from_slice(&rec[5..11]);
        out.push(AtaAttribute {
            id,
            current: rec[3],
            worst: rec[4],
            raw: u64::from_le_bytes(raw_bytes),
        });
    }
    out
}

/// The fields DiskPilot reads from the NVMe SMART/Health log page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeHealth {
    /// Percentage of the drive's rated life used (may exceed 100 on
    /// a drive run past its rating).
    pub percentage_used: u8,
    /// Composite temperature, Kelvin.
    pub temperature_k: i32,
    /// Available spare capacity, percent.
    pub available_spare: u8,
    pub power_on_hours: u64,
    pub power_cycles: u64,
    pub unsafe_shutdowns: u64,
    /// Media and data integrity errors over the drive's life.
    pub media_errors: u64,
}

/// Read the low 64 bits of a 128-bit little-endian counter.
fn u128_low(data: &[u8], off: usize) -> u64 {
    match data.get(off..off + 8) {
        Some(bytes) => {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(bytes);
            u64::from_le_bytes(buf)
        }
        None => 0,
    }
}

/// Parse the 512-byte NVMe SMART/Health log page. Fields whose bytes
/// fall outside the buffer read as zero; never panics.
pub fn parse_nvme_health(log: &[u8]) -> NvmeHealth {
    let temperature_k = match log.get(1..3) {
        Some(bytes) => i32::from(u16::from_le_bytes([bytes[0], bytes[1]])),
        None => 0,
    };
    NvmeHealth {
        percentage_used: log.get(5).copied().unwrap_or(0),
        temperature_k,
        available_spare: log.get(3).copied().unwrap_or(0),
        power_cycles: u128_low(log, 112),
        power_on_hours: u128_low(log, 128),
        unsafe_shutdowns: u128_low(log, 144),
        media_errors: u128_low(log, 160),
    }
}

/// Overall health verdict for a drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Healthy,
    Caution,
    Critical,
    Unknown,
}

/// One drive's health, in the shape the UI serves. Every vital is
/// optional: a drive (or a transport) that cannot report a value
/// says so honestly instead of inventing one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveHealth {
    pub model: String,
    /// Transport the health was read over, e.g. "NVMe", "SATA", "USB".
    pub interface: String,
    pub capacity_bytes: u64,
    pub temperature_c: Option<i32>,
    /// Estimated life remaining, percent (100 = new). Only set when
    /// the drive reports life semantics.
    pub life_left_percent: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    /// The drive's own SMART status reports impending failure.
    pub predicted_failure: bool,
    pub status: HealthStatus,
}

/// Classify vitals into a verdict. Pure and deliberately
/// conservative:
///
/// - Critical: predicted failure, any media errors, life left
///   <= 10%, or temperature >= 85 °C.
/// - Caution: life left <= 20%, temperature >= 70 °C, or any
///   reallocated sectors.
/// - Healthy: at least one vital is known and none of the above
///   fired.
/// - Unknown: nothing is known and no failure is predicted.
pub fn classify(
    life_left: Option<u32>,
    temp_c: Option<i32>,
    reallocated: Option<u64>,
    media_errors: Option<u64>,
    predicted_failure: bool,
) -> HealthStatus {
    if predicted_failure
        || media_errors.is_some_and(|m| m > 0)
        || life_left.is_some_and(|l| l <= 10)
        || temp_c.is_some_and(|t| t >= 85)
    {
        return HealthStatus::Critical;
    }
    if life_left.is_some_and(|l| l <= 20)
        || temp_c.is_some_and(|t| t >= 70)
        || reallocated.is_some_and(|r| r > 0)
    {
        return HealthStatus::Caution;
    }
    if life_left.is_none() && temp_c.is_none() && reallocated.is_none() && media_errors.is_none() {
        return HealthStatus::Unknown;
    }
    HealthStatus::Healthy
}

fn ata_attr(attrs: &[AtaAttribute], id: u8) -> Option<&AtaAttribute> {
    attrs.iter().find(|a| a.id == id)
}

/// Build a [`DriveHealth`] from ATA SMART attributes.
/// `predicted_failure` is the drive's own SMART status (the shell
/// reads it from the SMART status IOCTL alongside the attributes).
pub fn health_from_ata(
    model: &str,
    interface: &str,
    capacity_bytes: u64,
    attrs: &[AtaAttribute],
    predicted_failure: bool,
) -> DriveHealth {
    // Temperature: attribute 194 (Temperature Celsius), first raw
    // byte. Attribute 231 carries temperature on some drives and is
    // the fallback. A zero byte means "not reported", not 0 °C.
    let temperature_c = ata_attr(attrs, 194)
        .map(|a| a.raw & 0xFF)
        .filter(|t| *t > 0)
        .or_else(|| {
            ata_attr(attrs, 231)
                .map(|a| a.raw & 0xFF)
                .filter(|t| *t > 0)
        })
        .map(|t| t as i32);
    // Life left: attribute 231's *normalized* value when a drive
    // reports it with SSD-life semantics (100 = new .. 0 = spent);
    // values outside 1..=100 are not life readings and are ignored.
    // Attribute 231 is dual-use across vendors — temperature on some
    // drives, SSD life left on others — so a single 231 reading can
    // never be read as both at once. Life is only sourced from 231
    // when attribute 194 is present and temperature is independently
    // secured; when 194 is absent, 231's raw byte is the temperature
    // fallback above and its normalized value means nothing here.
    let life_left_percent = if ata_attr(attrs, 194).is_some() {
        ata_attr(attrs, 231)
            .map(|a| u32::from(a.current))
            .filter(|v| (1..=100).contains(v))
    } else {
        None
    };
    let reallocated_sectors = ata_attr(attrs, 5).map(|a| a.raw);
    let power_on_hours = ata_attr(attrs, 9).map(|a| a.raw);
    let power_cycles = ata_attr(attrs, 12).map(|a| a.raw);
    DriveHealth {
        model: model.to_string(),
        interface: interface.to_string(),
        capacity_bytes,
        temperature_c,
        life_left_percent,
        power_on_hours,
        power_cycles,
        reallocated_sectors,
        predicted_failure,
        status: classify(
            life_left_percent,
            temperature_c,
            reallocated_sectors,
            None,
            predicted_failure,
        ),
    }
}

/// Build a [`DriveHealth`] from a parsed NVMe health log.
pub fn health_from_nvme(
    model: &str,
    interface: &str,
    capacity_bytes: u64,
    health: &NvmeHealth,
) -> DriveHealth {
    let temperature_c = if health.temperature_k > 0 {
        Some(health.temperature_k - 273)
    } else {
        None
    };
    // Percentage used may exceed 100 on an over-run drive; life left
    // saturates at 0 rather than going negative.
    let life_left_percent = Some(100u32.saturating_sub(u32::from(health.percentage_used)));
    DriveHealth {
        model: model.to_string(),
        interface: interface.to_string(),
        capacity_bytes,
        temperature_c,
        life_left_percent,
        power_on_hours: Some(health.power_on_hours),
        power_cycles: Some(health.power_cycles),
        reallocated_sectors: None,
        predicted_failure: false,
        status: classify(
            life_left_percent,
            temperature_c,
            None,
            Some(health.media_errors),
            false,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ata_buffer() -> Vec<u8> {
        let mut data = vec![0u8; 512];
        data[0] = 0x10; // revision low byte
        let mut write_attr = |idx: usize, id: u8, current: u8, worst: u8, raw: u64| {
            let off = 2 + idx * 12;
            data[off] = id;
            data[off + 3] = current;
            data[off + 4] = worst;
            data[off + 5..off + 11].copy_from_slice(&raw.to_le_bytes()[..6]);
        };
        write_attr(0, 5, 100, 100, 7); // reallocated sectors: raw 7
        write_attr(1, 9, 98, 98, 12_345); // power-on hours
        write_attr(2, 12, 99, 99, 432); // power cycles
        write_attr(3, 194, 64, 40, 41); // temperature 41 C
        // Life-left semantics: normalized 87. The raw byte is 0 (not
        // reported): temperature comes from 194 above, never from
        // this same attribute.
        write_attr(4, 231, 87, 87, 0);
        data
    }

    #[test]
    fn ata_parser_reads_offsets_and_skips_empty_slots() {
        let attrs = parse_ata_attributes(&ata_buffer());
        assert_eq!(attrs.len(), 5);
        assert_eq!(
            attrs[0],
            AtaAttribute {
                id: 5,
                current: 100,
                worst: 100,
                raw: 7
            }
        );
        assert_eq!(attrs[1].raw, 12_345);
        assert_eq!(attrs[3].id, 194);
        assert_eq!(attrs[3].raw, 41);
        assert_eq!(attrs[4].current, 87);
    }

    #[test]
    fn ata_parser_never_panics_on_any_truncation() {
        let full = ata_buffer();
        for len in 0..=full.len() {
            let attrs = parse_ata_attributes(&full[..len]);
            // Complete records only: floor((len-2)/12), capped at 30,
            // minus none (fixture uses its first slots contiguously).
            let expected = len.saturating_sub(2) / 12;
            assert_eq!(attrs.len(), expected.min(5), "len {len}");
        }
        assert!(parse_ata_attributes(&[]).is_empty());
        assert!(parse_ata_attributes(&[0xFF; 1]).is_empty());
    }

    #[test]
    fn ata_health_maps_vitals() {
        let attrs = parse_ata_attributes(&ata_buffer());
        let h = health_from_ata("Samsung SSD 870", "SATA", 500_000_000_000, &attrs, false);
        assert_eq!(h.temperature_c, Some(41));
        assert_eq!(h.life_left_percent, Some(87));
        assert_eq!(h.reallocated_sectors, Some(7));
        assert_eq!(h.power_on_hours, Some(12_345));
        assert_eq!(h.power_cycles, Some(432));
        // Reallocated sectors > 0 -> Caution.
        assert_eq!(h.status, HealthStatus::Caution);
    }

    #[test]
    fn ata_health_no_attrs_is_unknown() {
        let h = health_from_ata("Mystery", "USB", 0, &[], false);
        assert_eq!(h.status, HealthStatus::Unknown);
        assert_eq!(h.temperature_c, None);
        // A predicted failure alone is Critical even with no vitals.
        let h2 = health_from_ata("Mystery", "USB", 0, &[], true);
        assert_eq!(h2.status, HealthStatus::Critical);
    }

    #[test]
    fn ata_231_alone_is_temperature_never_life() {
        // Panel repro: a drive reporting ONLY attribute 231, with
        // temperature semantics (raw low byte 45). Sourcing life
        // from the same reading would take the normalized value 10
        // as "10% life left" and call this healthy-temperature
        // drive Critical. 231 alone is temperature, never life.
        let attrs = vec![AtaAttribute {
            id: 231,
            current: 10,
            worst: 10,
            raw: 45,
        }];
        let h = health_from_ata("Solo 231", "SATA", 0, &attrs, false);
        assert_eq!(h.temperature_c, Some(45));
        assert_eq!(h.life_left_percent, None);
        assert_ne!(h.status, HealthStatus::Critical);
        assert_eq!(h.status, HealthStatus::Healthy);
    }

    #[test]
    fn ata_life_left_ignores_out_of_range_normalized_values() {
        let attrs = vec![AtaAttribute {
            id: 231,
            current: 253,
            worst: 253,
            raw: 30,
        }];
        let h = health_from_ata("X", "SATA", 0, &attrs, false);
        assert_eq!(h.life_left_percent, None);
        // 231's raw low byte still serves as the temperature fallback.
        assert_eq!(h.temperature_c, Some(30));
    }

    fn nvme_log() -> Vec<u8> {
        let mut log = vec![0u8; 512];
        log[1..3].copy_from_slice(&318u16.to_le_bytes()); // 318 K = 45 C
        log[3] = 96; // available spare
        log[5] = 12; // percentage used
        log[112..120].copy_from_slice(&321u64.to_le_bytes()); // power cycles
        log[128..136].copy_from_slice(&54_321u64.to_le_bytes()); // power-on hours
        log[144..152].copy_from_slice(&17u64.to_le_bytes()); // unsafe shutdowns
        log[160..168].copy_from_slice(&0u64.to_le_bytes()); // media errors
        log
    }

    #[test]
    fn nvme_parser_reads_spec_offsets() {
        let h = parse_nvme_health(&nvme_log());
        assert_eq!(h.temperature_k, 318);
        assert_eq!(h.available_spare, 96);
        assert_eq!(h.percentage_used, 12);
        assert_eq!(h.power_cycles, 321);
        assert_eq!(h.power_on_hours, 54_321);
        assert_eq!(h.unsafe_shutdowns, 17);
        assert_eq!(h.media_errors, 0);
    }

    #[test]
    fn nvme_parser_never_panics_on_truncation() {
        let full = nvme_log();
        for len in 0..=full.len() {
            let _ = parse_nvme_health(&full[..len]);
        }
        let h = parse_nvme_health(&full[..100]);
        assert_eq!(h.temperature_k, 318);
        assert_eq!(h.power_cycles, 0); // bytes 112.. are out of range
    }

    #[test]
    fn nvme_health_maps_vitals_and_clamps_life() {
        let h = health_from_nvme(
            "WD Black SN850X",
            "NVMe",
            1_000_000_000_000,
            &parse_nvme_health(&nvme_log()),
        );
        assert_eq!(h.temperature_c, Some(45));
        assert_eq!(h.life_left_percent, Some(88));
        assert_eq!(h.status, HealthStatus::Healthy);
        let spent = NvmeHealth {
            percentage_used: 140,
            ..parse_nvme_health(&nvme_log())
        };
        let h2 = health_from_nvme("Old", "NVMe", 0, &spent);
        assert_eq!(h2.life_left_percent, Some(0));
        assert_eq!(h2.status, HealthStatus::Critical);
        let errs = NvmeHealth {
            media_errors: 3,
            ..parse_nvme_health(&nvme_log())
        };
        assert_eq!(
            health_from_nvme("Old", "NVMe", 0, &errs).status,
            HealthStatus::Critical
        );
    }

    #[test]
    fn classify_boundaries() {
        // (life, temp, realloc, media, predicted) -> expected
        type Case = (
            Option<u32>,
            Option<i32>,
            Option<u64>,
            Option<u64>,
            bool,
            HealthStatus,
        );
        let cases: &[Case] = &[
            (None, None, None, None, true, HealthStatus::Critical),
            (None, None, None, Some(1), false, HealthStatus::Critical),
            (Some(10), None, None, None, false, HealthStatus::Critical),
            (None, Some(85), None, None, false, HealthStatus::Critical),
            (Some(11), None, None, None, false, HealthStatus::Caution),
            (Some(20), None, None, None, false, HealthStatus::Caution),
            (None, Some(70), None, None, false, HealthStatus::Caution),
            (None, Some(84), None, None, false, HealthStatus::Caution),
            (None, None, Some(1), None, false, HealthStatus::Caution),
            (
                Some(21),
                Some(69),
                Some(0),
                Some(0),
                false,
                HealthStatus::Healthy,
            ),
            (
                Some(100),
                Some(30),
                Some(0),
                Some(0),
                false,
                HealthStatus::Healthy,
            ),
            // One known, fine vital is enough for Healthy.
            (None, Some(45), None, None, false, HealthStatus::Healthy),
            (None, None, Some(0), None, false, HealthStatus::Healthy),
            // Nothing known -> Unknown.
            (None, None, None, None, false, HealthStatus::Unknown),
        ];
        for (life, temp, realloc, media, predicted, expected) in cases {
            assert_eq!(
                classify(*life, *temp, *realloc, *media, *predicted),
                *expected,
                "case life={life:?} temp={temp:?} realloc={realloc:?} media={media:?} predicted={predicted}"
            );
        }
    }
}
