//! Exact constancy of complete component tuples in a decoded image.

pub(crate) fn constant(bytes: &[u8], components: usize) -> bool {
    // A nonempty sequence of complete pixels is constant exactly when removing
    // its first pixel and removing its last pixel yield identical byte slices.
    // Whole-slice equality avoids a separate comparison for every pixel.
    components > 0
        && bytes.len() >= components
        && bytes.len() % components == 0
        && bytes[components..] == bytes[..bytes.len() - components]
}

#[cfg(test)]
mod tests {
    use super::constant;

    #[test]
    fn short_buffers_match_independent_component_indexing() {
        // Enumerate every buffer over three sample values, including empty,
        // incomplete, one-pixel and nonuniform images, with varying strides.
        for len in 0..=8 {
            for mut code in 0..3usize.pow(len as u32) {
                let bytes: Vec<_> = (0..len)
                    .map(|_| {
                        let sample = [0, 39, 255][code % 3];
                        code /= 3;
                        sample
                    })
                    .collect();
                for components in 0..=10 {
                    let expected = components != 0
                        && len >= components
                        && len % components == 0
                        && (0..len).all(|i| bytes[i] == bytes[i % components]);
                    assert_eq!(
                        constant(&bytes, components),
                        expected,
                        "{bytes:?}, stride {components}"
                    );
                }
            }
        }
    }

    #[test]
    fn complete_image_including_first_and_last_samples_is_checked() {
        for components in 1..=8 {
            let pixel: Vec<_> = (0..components).map(|i| (i * 31 + 17) as u8).collect();
            let bytes = pixel.repeat(4096);
            assert!(constant(&bytes, components));
            for at in [0, components - 1, bytes.len() / 2, bytes.len() - 1] {
                let mut changed = bytes.clone();
                changed[at] ^= 1;
                assert!(!constant(&changed, components));
            }
            for remainder in 1..components {
                assert!(!constant(&bytes[..bytes.len() - remainder], components));
            }
        }
        assert!(!constant(&[17], usize::MAX));
    }
}
