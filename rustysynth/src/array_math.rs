#[non_exhaustive]
pub(crate) struct ArrayMath {}

impl ArrayMath {
    pub(crate) fn multiply_add(a: f32, x: &[f32], destination: &mut [f32]) {
        for (x, destination) in x.iter().zip(destination.iter_mut()) {
            *destination += a * *x;
        }
    }

    pub(crate) fn multiply_add_slope(a: f32, step: f32, x: &[f32], destination: &mut [f32]) {
        let mut a = a;
        for (x, destination) in x.iter().zip(destination.iter_mut()) {
            *destination += a * *x;
            a += step;
        }
    }

    /// Adds the scaled source to both destinations in a single pass.
    ///
    /// The left and right channel of a voice share the same source block, so
    /// mixing them separately walked the source twice for no reason.
    pub(crate) fn multiply_add_stereo(
        a_left: f32,
        a_right: f32,
        x: &[f32],
        destination_left: &mut [f32],
        destination_right: &mut [f32],
    ) {
        let destinations = destination_left
            .iter_mut()
            .zip(destination_right.iter_mut());
        for (x, (destination_left, destination_right)) in x.iter().zip(destinations) {
            *destination_left += a_left * *x;
            *destination_right += a_right * *x;
        }
    }

    /// Adds the linearly ramped source to both destinations in a single pass.
    ///
    /// A channel whose two gains are equal is given a zero step, which keeps its
    /// gain constant and therefore produces the very same products as
    /// [ArrayMath::multiply_add_stereo] would.
    pub(crate) fn multiply_add_slope_stereo(
        a_left: f32,
        step_left: f32,
        a_right: f32,
        step_right: f32,
        x: &[f32],
        destination_left: &mut [f32],
        destination_right: &mut [f32],
    ) {
        let mut a_left = a_left;
        let mut a_right = a_right;

        let destinations = destination_left
            .iter_mut()
            .zip(destination_right.iter_mut());
        for (x, (destination_left, destination_right)) in x.iter().zip(destinations) {
            *destination_left += a_left * *x;
            *destination_right += a_right * *x;
            a_left += step_left;
            a_right += step_right;
        }
    }
}
