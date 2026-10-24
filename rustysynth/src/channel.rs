#[derive(Debug, PartialEq, Eq)]
enum DataType {
    None,
    Rpn,
    Nrpn,
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct Channel {
    pub(crate) is_percussion_channel: bool,

    bank_number: i32,
    patch_number: i32,

    modulation: i16,
    volume: i16,
    pan: i16,
    expression: i16,
    hold_pedal: bool,

    reverb_send: u8,
    chorus_send: u8,

    rpn: i16,
    pitch_bend_range: i16,
    coarse_tune: i16,
    fine_tune: i16,

    pitch_bend: f32,

    last_data_type: DataType,
}

impl Channel {
    pub(crate) fn new(is_percussion_channel: bool) -> Self {
        let mut channel = Self {
            is_percussion_channel,
            bank_number: 0,
            patch_number: 0,
            modulation: 0,
            volume: 0,
            pan: 0,
            expression: 0,
            hold_pedal: false,
            reverb_send: 0,
            chorus_send: 0,
            rpn: 0,
            pitch_bend_range: 0,
            coarse_tune: 0,
            fine_tune: 0,
            pitch_bend: 0_f32,
            last_data_type: DataType::None,
        };

        channel.reset();

        channel
    }

    pub(crate) fn reset(&mut self) {
        self.bank_number = if self.is_percussion_channel { 128 } else { 0 };
        self.patch_number = 0;

        self.modulation = 0;
        self.volume = 100 << 7;
        self.pan = 64 << 7;
        self.expression = 127 << 7;
        self.hold_pedal = false;

        self.reverb_send = 40;
        self.chorus_send = 0;

        self.rpn = -1;
        self.pitch_bend_range = 2 << 7;
        self.coarse_tune = 0;
        self.fine_tune = 8192;

        self.pitch_bend = 0_f32;
    }

    pub(crate) fn reset_all_controllers(&mut self) {
        self.modulation = 0;
        self.expression = 127 << 7;
        self.hold_pedal = false;

        self.rpn = -1;

        self.pitch_bend = 0_f32;
    }

    pub(crate) fn set_bank(&mut self, value: i32) {
        self.bank_number = Channel::clamp_data(value);

        if self.is_percussion_channel {
            self.bank_number += 128;
        }
    }

    pub(crate) fn set_patch(&mut self, value: i32) {
        self.patch_number = Channel::clamp_data(value);
    }

    // The MIDI specification gives every data byte 7 bits. A caller of
    // Synthesizer::process_midi_message can pass anything, though, and shifting
    // such a value by 7 bits or narrowing it to u8 would silently wrap around
    // and change the sound in unrelated ways.
    // https://github.com/sinshu/rustysynth/issues/57
    fn clamp_data(value: i32) -> i32 {
        value.clamp(0, 127)
    }

    pub(crate) fn set_modulation_coarse(&mut self, value: i32) {
        self.modulation = (self.modulation & 0x7F) | (Channel::clamp_data(value) << 7) as i16;
    }

    pub(crate) fn set_modulation_fine(&mut self, value: i32) {
        self.modulation = (((self.modulation as i32) & 0xFF80) | Channel::clamp_data(value)) as i16;
    }

    pub(crate) fn set_volume_coarse(&mut self, value: i32) {
        self.volume = (self.volume & 0x7F) | (Channel::clamp_data(value) << 7) as i16;
    }

    pub(crate) fn set_volume_fine(&mut self, value: i32) {
        self.volume = (((self.volume as i32) & 0xFF80) | Channel::clamp_data(value)) as i16;
    }

    pub(crate) fn set_pan_coarse(&mut self, value: i32) {
        self.pan = (self.pan & 0x7F) | (Channel::clamp_data(value) << 7) as i16;
    }

    pub(crate) fn set_pan_fine(&mut self, value: i32) {
        self.pan = (((self.pan as i32) & 0xFF80) | Channel::clamp_data(value)) as i16;
    }

    pub(crate) fn set_expression_coarse(&mut self, value: i32) {
        self.expression = (self.expression & 0x7F) | (Channel::clamp_data(value) << 7) as i16;
    }

    pub(crate) fn set_expression_fine(&mut self, value: i32) {
        self.expression = (((self.expression as i32) & 0xFF80) | Channel::clamp_data(value)) as i16;
    }

    pub(crate) fn set_hold_pedal(&mut self, value: i32) {
        self.hold_pedal = value >= 64;
    }

    pub(crate) fn set_reverb_send(&mut self, value: i32) {
        self.reverb_send = Channel::clamp_data(value) as u8;
    }

    pub(crate) fn set_chorus_send(&mut self, value: i32) {
        self.chorus_send = Channel::clamp_data(value) as u8;
    }

    pub(crate) fn set_rpn_coarse(&mut self, value: i32) {
        self.rpn = (self.rpn & 0x7F) | (Channel::clamp_data(value) << 7) as i16;
        self.last_data_type = DataType::Rpn;
    }

    pub(crate) fn set_rpn_fine(&mut self, value: i32) {
        self.rpn = (((self.rpn as i32) & 0xFF80) | Channel::clamp_data(value)) as i16;
        self.last_data_type = DataType::Rpn;
    }

    pub(crate) fn set_nrpn_coarse(&mut self, _value: i32) {
        self.last_data_type = DataType::Nrpn;
    }

    pub(crate) fn set_nrpn_fine(&mut self, _value: i32) {
        self.last_data_type = DataType::Nrpn;
    }

    pub(crate) fn data_entry_coarse(&mut self, value: i32) {
        if self.last_data_type != DataType::Rpn {
            return;
        }

        if self.rpn == 0 {
            self.pitch_bend_range =
                (self.pitch_bend_range & 0x7F) | (Channel::clamp_data(value) << 7) as i16;
        } else if self.rpn == 1 {
            self.fine_tune = (self.fine_tune & 0x7F) | (Channel::clamp_data(value) << 7) as i16;
        } else if self.rpn == 2 {
            self.coarse_tune = (Channel::clamp_data(value) - 64) as i16;
        }
    }

    pub(crate) fn data_entry_fine(&mut self, value: i32) {
        if self.last_data_type != DataType::Rpn {
            return;
        }

        if self.rpn == 0 {
            self.pitch_bend_range =
                (((self.pitch_bend_range as i32) & 0xFF80) | Channel::clamp_data(value)) as i16;
        } else if self.rpn == 1 {
            self.fine_tune =
                (((self.fine_tune as i32) & 0xFF80) | Channel::clamp_data(value)) as i16;
        }
    }

    pub(crate) fn set_pitch_bend(&mut self, value1: i32, value2: i32) {
        let value = (Channel::clamp_data(value1) | (Channel::clamp_data(value2) << 7)) - 8192;
        self.pitch_bend = (1_f32 / 8192_f32) * value as f32;
    }

    pub(crate) fn get_bank_number(&self) -> i32 {
        self.bank_number
    }

    pub(crate) fn get_patch_number(&self) -> i32 {
        self.patch_number
    }

    pub(crate) fn get_modulation(&self) -> f32 {
        (50_f32 / 16383_f32) * self.modulation as f32
    }

    pub(crate) fn get_volume(&self) -> f32 {
        (1_f32 / 16383_f32) * self.volume as f32
    }

    pub(crate) fn get_pan(&self) -> f32 {
        (100_f32 / 16383_f32) * self.pan as f32 - 50_f32
    }

    pub(crate) fn get_expression(&self) -> f32 {
        (1_f32 / 16383_f32) * self.expression as f32
    }

    pub(crate) fn get_hold_pedal(&self) -> bool {
        self.hold_pedal
    }

    pub(crate) fn get_reverb_send(&self) -> f32 {
        (1_f32 / 127_f32) * self.reverb_send as f32
    }

    pub(crate) fn get_chorus_send(&self) -> f32 {
        (1_f32 / 127_f32) * self.chorus_send as f32
    }

    pub(crate) fn get_pitch_bend_range(&self) -> f32 {
        (self.pitch_bend_range >> 7) as f32 + 0.01_f32 * (self.pitch_bend_range & 0x7F) as f32
    }

    pub(crate) fn get_tune(&self) -> f32 {
        self.coarse_tune as f32 + (1_f32 / 8192_f32) * (self.fine_tune - 8192) as f32
    }

    pub(crate) fn get_pitch_bend(&self) -> f32 {
        self.get_pitch_bend_range() * self.pitch_bend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The MIDI specification gives every data byte 7 bits, but a caller of
    // Synthesizer::process_midi_message can pass any i32. Out of range values
    // used to wrap around when they were shifted or narrowed (#57).
    #[test]
    fn test_data_bytes_are_clamped_to_seven_bits() {
        let mut channel = Channel::new(false);

        channel.set_bank(999);
        assert_eq!(channel.get_bank_number(), 127);
        channel.set_bank(-1);
        assert_eq!(channel.get_bank_number(), 0);

        channel.set_patch(300);
        assert_eq!(channel.get_patch_number(), 127);

        channel.set_reverb_send(999);
        assert_eq!(channel.get_reverb_send(), 1.0);
        channel.set_reverb_send(-1);
        assert_eq!(channel.get_reverb_send(), 0.0);
    }

    fn select_rpn(channel: &mut Channel, rpn: i32) {
        channel.set_rpn_coarse(0);
        channel.set_rpn_fine(rpn);
    }

    // A data entry value out of range must not wrap around when it is shifted
    // into the high half of a 14-bit register or narrowed to 7 bits.
    #[test]
    fn test_data_entry_is_clamped_to_seven_bits() {
        // RPN 0 is the pitch bend range.
        let mut channel = Channel::new(false);
        select_rpn(&mut channel, 0);
        channel.data_entry_coarse(999);
        assert_eq!(channel.get_pitch_bend_range(), 127.0);

        let mut channel = Channel::new(false);
        select_rpn(&mut channel, 0);
        channel.data_entry_coarse(-1);
        assert_eq!(channel.get_pitch_bend_range(), 0.0);

        // RPN 1 is the fine tune, stored as a signed offset from 8192.
        let mut channel = Channel::new(false);
        select_rpn(&mut channel, 1);
        channel.data_entry_coarse(999);
        assert_eq!(channel.get_tune(), (127.0 * 128.0 - 8192.0) / 8192.0);

        let mut channel = Channel::new(false);
        select_rpn(&mut channel, 1);
        channel.data_entry_coarse(-1);
        assert_eq!(channel.get_tune(), -1.0);

        // RPN 2 is the coarse tune, which is stored around a centre of 64.
        let mut channel = Channel::new(false);
        select_rpn(&mut channel, 2);
        channel.data_entry_coarse(999);
        assert_eq!(channel.get_tune(), 63.0);

        let mut channel = Channel::new(false);
        select_rpn(&mut channel, 2);
        channel.data_entry_coarse(-1);
        assert_eq!(channel.get_tune(), -64.0);
    }

    #[test]
    fn test_pitch_bend_is_clamped_to_seven_bits() {
        let mut channel = Channel::new(false);

        channel.set_pitch_bend(127, 127);
        assert_eq!(channel.get_pitch_bend(), 2.0 * 8191.0 / 8192.0);

        channel.set_pitch_bend(999, 999);
        assert_eq!(channel.get_pitch_bend(), 2.0 * 8191.0 / 8192.0);

        channel.set_pitch_bend(-1, -1);
        assert_eq!(channel.get_pitch_bend(), -2.0);
    }
}
