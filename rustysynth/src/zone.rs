use crate::error::SoundFontError;
use crate::generator::Generator;
use crate::zone_info::ZoneInfo;

#[non_exhaustive]
pub(crate) struct Zone {
    pub(crate) generators: Vec<Generator>,
}

impl Zone {
    pub(crate) fn empty() -> Self {
        Self {
            generators: Vec::new(),
        }
    }

    fn new(info: &ZoneInfo, generators: &[Generator]) -> Self {
        // Copy the zone's generator span exactly once, with a single allocation.
        let start = info.generator_index as usize;
        let count = info.generator_count as usize;
        let mut segment: Vec<Generator> = Vec::with_capacity(count);
        segment.extend_from_slice(&generators[start..start + count]);

        Self {
            generators: segment,
        }
    }

    pub(crate) fn create(
        infos: &[ZoneInfo],
        generators: &[Generator],
    ) -> Result<Vec<Zone>, SoundFontError> {
        if infos.len() <= 1 {
            return Err(SoundFontError::ZoneNotFound);
        }

        // The last one is the terminator.
        let count = infos.len() - 1;

        let mut zones: Vec<Zone> = Vec::with_capacity(count);
        for info in infos.iter().take(count) {
            zones.push(Zone::new(info, generators));
        }

        Ok(zones)
    }
}
