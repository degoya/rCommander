use super::*;

/// A 2x2 bitmap in BGRA: blue, green, red, white.
fn bitmap(alpha: [u8; 4]) -> Vec<u8> {
    vec![
        255, 0, 0, alpha[0], //
        0, 255, 0, alpha[1], //
        0, 0, 255, alpha[2], //
        255, 255, 255, alpha[3],
    ]
}

#[test]
fn an_icon_with_alpha_keeps_it_and_swaps_blue_and_red() {
    let color = bitmap([255, 128, 0, 10]);
    let rgba = from_bgra(2, 2, &color, Some(&[255; 16])).expect("the sizes match");
    assert_eq!((rgba.width, rgba.height), (2, 2));
    assert_eq!(
        rgba.pixels,
        vec![
            0, 0, 255, 255, //
            0, 255, 0, 128, //
            255, 0, 0, 0, //
            255, 255, 255, 10,
        ],
        "the alpha channel wins over the mask"
    );
}

#[test]
fn an_icon_without_alpha_takes_its_transparency_from_the_mask() {
    let color = bitmap([0; 4]);
    // Transparent where the mask is white: the first and the last pixel.
    let mask = vec![
        255, 255, 255, 0, //
        0, 0, 0, 0, //
        0, 0, 0, 0, //
        255, 255, 255, 0,
    ];
    let rgba = from_bgra(2, 2, &color, Some(&mask)).expect("the sizes match");
    let alphas: Vec<u8> = rgba
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[3])
        .collect();
    assert_eq!(alphas, vec![0, 255, 255, 0]);
}

#[test]
fn an_icon_without_alpha_or_mask_is_opaque() {
    let rgba = from_bgra(2, 2, &bitmap([0; 4]), None).expect("the sizes match");
    assert!(rgba.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
}

#[test]
fn buffers_that_do_not_fit_the_size_are_refused() {
    assert_eq!(from_bgra(3, 2, &bitmap([0; 4]), None), None);
    assert_eq!(from_bgra(2, 2, &bitmap([0; 4]), Some(&[0; 4])), None);
}
