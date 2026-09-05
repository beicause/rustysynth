mod instrument_util;
mod preset_util;
mod sample_util;

mod timgm6mb_info_test;
mod timgm6mb_instrument_test;
mod timgm6mb_preset_test;
mod timgm6mb_sample_test;

mod fluidr3mono_info_test;
mod fluidr3mono_instrument_test;
mod fluidr3mono_preset_test;
mod fluidr3mono_sample_test;

#[cfg(feature = "sf3")]
mod fluidr3mono_sf3_info_test;
#[cfg(feature = "sf3")]
mod fluidr3mono_sf3_instrument_test;
#[cfg(feature = "sf3")]
mod fluidr3mono_sf3_preset_test;
#[cfg(feature = "sf3")]
mod fluidr3mono_sf3_sample_test;
