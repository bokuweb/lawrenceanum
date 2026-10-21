use super::*;
fn idx(parts: &[Vec<u8>]) -> Vec<u8> {
    if parts.is_empty() {
        return vec![0, 0];
    }
    let mut o = (parts.len() as u16).to_be_bytes().to_vec();
    o.push(2);
    let mut n = 1u16;
    o.extend(n.to_be_bytes());
    for p in parts {
        n += p.len() as u16;
        o.extend(n.to_be_bytes())
    }
    for p in parts {
        o.extend(p)
    }
    o
}
fn num(n: i32) -> Vec<u8> {
    let mut a = vec![29];
    a.extend(n.to_be_bytes());
    a
}
fn font(reg: &str, order: &str, supp: i32, cid: u16, format: u8) -> Vec<u8> {
    let names = idx(&[b"ArbitraryEmbeddedFont".to_vec()]);
    let strings = idx(&[reg.as_bytes().to_vec(), order.as_bytes().to_vec()]);
    let charset = match format {
        0 => vec![0, (cid >> 8) as u8, cid as u8],
        1 => vec![1, (cid >> 8) as u8, cid as u8, 0],
        2 => vec![2, (cid >> 8) as u8, cid as u8, 0, 0],
        _ => vec![format],
    };
    let top = |c: i32, cs: i32, fd: i32, fds: i32| {
        let mut d = Vec::new();
        for n in [391, 392, supp] {
            d.extend(num(n));
        }
        d.extend([12, 30]);
        for (n, op) in [
            (c, vec![15]),
            (cs, vec![17]),
            (fd, vec![12, 36]),
            (fds, vec![12, 37]),
        ] {
            d.extend(num(n));
            d.extend(op);
        }
        d
    };
    let cs = idx(&[vec![14], vec![14]]);
    let fd = idx(&[[num(0), num(0), vec![18]].concat()]);
    let head = vec![1, 0, 4, 4];
    let start =
        (head.len() + names.len() + idx(&[top(0, 0, 0, 0)]).len() + strings.len() + 2) as i32;
    let csp = start + charset.len() as i32;
    let fdp = csp + cs.len() as i32;
    let fdsp = fdp + fd.len() as i32;
    [
        head,
        names,
        idx(&[top(start, csp, fdp, fdsp)]),
        strings,
        vec![0, 0],
        charset,
        cs,
        fd,
        vec![0, 0, 0],
    ]
    .concat()
}

#[test]
fn three_charset_formats_preserve_explicit_CIDs() {
    for format in [0, 1, 2] {
        assert_eq!(
            standard_cids(&font("Adobe", "Japan1", 6, 17518, format)),
            Some(vec![0, 17518])
        );
    }
}
#[test]
fn arbitrary_fontname_is_not_an_identity_proof() {
    let a = font("Private", "Japan1", 6, 17518, 0);
    assert!(standard_cids(&a).is_none());
}
#[test]
fn foreign_ROS_and_unknown_supplement_refused() {
    for (reg, order, supp) in [
        ("Adobe", "GB1", 6),
        ("Adobe", "Identity", 6),
        ("Private", "Japan1", 6),
        ("Adobe", "Japan1", 8),
    ] {
        assert!(standard_cids(&font(reg, order, supp, 17518, 0)).is_none());
    }
}
#[test]
fn truncated_every_prefix_is_refused_without_panics() {
    let a = font("Adobe", "Japan1", 6, 17518, 0);
    for n in 0..a.len() {
        assert!(standard_cids(&a[..n]).is_none());
    }
    for n in 0..4 {
        assert!(standard_cids(&a[..n]).is_none());
    }
}
#[test]
fn CFF2_bad_header_and_bad_offsize_refused() {
    for (i, v) in [(0, 2), (2, 1), (3, 0), (6, 0)] {
        let mut a = font("Adobe", "Japan1", 6, 17518, 0);
        a[i] = v;
        assert!(standard_cids(&a).is_none());
    }
}
#[test]
fn duplicate_notdef_CID_and_unknown_charset_format_refused() {
    assert!(standard_cids(&font("Adobe", "Japan1", 6, 0, 0)).is_none());
    assert!(standard_cids(&font("Adobe", "Japan1", 6, 17518, 3)).is_none());
}
#[test]
fn extended_CID_can_exist_but_does_not_create_an_AJ1_lookup() {
    let a = font("Adobe", "Japan1", 6, 25937, 0);
    assert_eq!(standard_cids(&a), Some(vec![0, 25937]));
    assert_eq!(crate::font::aj1(25937), None);
}

fn tables(data: &[u8]) -> (usize, usize, usize, usize) {
    let mut p = data[2] as usize;
    let _ = index(data, &mut p).unwrap();
    let top = index(data, &mut p).unwrap();
    let d = dict(top[0]).unwrap();
    let get = |op| d[&op][0].unwrap() as usize;
    (get(15), get(17), get(0x0c24), get(0x0c25))
}
#[test]
fn all_GIDs_need_an_existing_Font_DICT_including_notdef() {
    for gid in [0, 1] {
        let mut a = font("Adobe", "Japan1", 6, 17518, 0);
        let (_, _, _, fds) = tables(&a);
        a[fds + 1 + gid] = 1;
        assert!(standard_cids(&a).is_none(), "invalid FD for GID {gid}");
    }
}
#[test]
fn missing_FD_structures_have_no_source_authority() {
    assert!(standard_cids(include_bytes!(
        "source-structure-fixtures/missing-fd-structures.cff"
    ))
    .is_none());
}
#[test]
fn range3_FDSelect_covers_every_GID_and_has_exact_sentinel() {
    let a = font("Adobe", "Japan1", 6, 17518, 0);
    let (_, _, _, fds) = tables(&a);
    let good = [3, 0, 1, 0, 0, 0, 0, 2];
    let mut b = a[..fds].to_vec();
    b.extend(good);
    assert_eq!(standard_cids(&b), Some(vec![0, 17518]));
    for tail in [
        vec![3, 0, 0, 0, 2],
        vec![3, 0, 1, 0, 1, 0, 0, 2],
        vec![3, 0, 1, 0, 0, 1, 0, 2],
        vec![3, 0, 1, 0, 0, 0, 0, 1],
        vec![3, 0, 1, 0, 0, 0, 0, 3],
        vec![3, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2],
        vec![4, 0, 0],
    ] {
        let mut b = a[..fds].to_vec();
        b.extend(tail);
        assert!(standard_cids(&b).is_none(), "malformed range3: {b:?}");
    }
}
#[test]
fn Font_DICT_requires_one_in_bounds_Private_dictionary() {
    let a = font("Adobe", "Japan1", 6, 17518, 0);
    let (_, _, fd, _) = tables(&a);
    let mut p = fd;
    let ds = index(&a, &mut p).unwrap();
    let off = ds[0].as_ptr() as usize - a.as_ptr() as usize;
    for args in [[-1, 0], [10, i32::MAX], [1, -1], [1, 0]] {
        let mut b = a.clone();
        b[off..off + 5].copy_from_slice(&num(args[0]));
        b[off + 5..off + 10].copy_from_slice(&num(args[1]));
        assert!(standard_cids(&b).is_none(), "Private size/offset {args:?}");
    }
    let mut b = a.clone();
    b[off + 10] = 17;
    assert!(standard_cids(&b).is_none(), "missing Private operator");
}
#[test]
fn empty_notdef_charstring_is_not_an_original_glyph() {
    let mut a = font("Adobe", "Japan1", 6, 17518, 0);
    let (_, cs, _, _) = tables(&a);
    a[cs + 5..cs + 7].copy_from_slice(&[0, 1]);
    assert!(standard_cids(&a).is_none());
}
fn set_top_offset(data: &mut [u8], op: [u8; 2], value: usize) {
    let mut p = data[2] as usize;
    index(data, &mut p).unwrap();
    let top = index(data, &mut p).unwrap();
    let d = top[0];
    let positions: Vec<_> = d
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| (w == op).then_some(i))
        .collect();
    assert_eq!(positions.len(), 1);
    let operator = d.as_ptr() as usize - data.as_ptr() as usize + positions[0];
    assert_eq!(data[operator - 5], 29);
    data[operator - 5..operator].copy_from_slice(&num(value.try_into().unwrap()));
}
#[test]
fn multiple_Font_DICTs_and_multiple_ranges_preserve_explicit_GID_routing() {
    for select in [vec![0, 1, 0], vec![3, 0, 2, 0, 0, 1, 0, 1, 0, 0, 2]] {
        let original = font("Adobe", "Japan1", 6, 17518, 0);
        let (_, _, fda, _) = tables(&original);
        let fd = [num(0), num(0), vec![18]].concat();
        let fonts = idx(&[fd.clone(), fd]);
        let mut a = original[..fda].to_vec();
        a.extend(&fonts);
        a.extend(&select);
        set_top_offset(&mut a, [12, 37], fda + fonts.len());
        assert_eq!(standard_cids(&a), Some(vec![0, 17518]));
    }
}
#[test]
fn Private_local_Subrs_are_relative_and_must_be_a_complete_INDEX() {
    let original = font("Adobe", "Japan1", 6, 17518, 0);
    let (_, _, fda, fds) = tables(&original);
    let private_offset = fds + 3;
    let mut base = original[..fda].to_vec();
    base.extend(idx(&[
        [num(6), num(private_offset as i32), vec![18]].concat()
    ]));
    base.extend([0, 0, 0]);
    assert_eq!(base.len(), private_offset);
    let private = [num(6), vec![19]].concat();
    let mut good = base.clone();
    good.extend(&private);
    good.extend(idx(&[vec![14]]));
    assert_eq!(standard_cids(&good), Some(vec![0, 17518]));
    for relative in [0, 5, i32::MAX] {
        let mut bad = base.clone();
        bad.extend(num(relative));
        bad.push(19);
        bad.extend(idx(&[vec![14]]));
        assert!(
            standard_cids(&bad).is_none(),
            "relative local Subrs {relative}"
        );
    }
    let mut bad = good.clone();
    bad.pop();
    assert!(standard_cids(&bad).is_none(), "truncated local Subrs INDEX");
}
#[test]
fn DICT_real_operands_require_a_complete_finite_number() {
    for real in [
        vec![0x0a, 0x03, 0x9f],
        vec![0xea, 0x25, 0xc3, 0xff],
        vec![0x1b, 0x3f],
    ] {
        let mut d = vec![30];
        d.extend(real);
        d.extend([12, 9]);
        assert!(dict(&d).is_some());
    }
    for real in [
        vec![0x1a, 0x2a, 0x3f],
        vec![0xef],
        vec![0x1b, 0xff],
        vec![0x1b, 0x1b, 0x2f],
        vec![0x1b, 0x99, 0x99, 0x9f],
        vec![0xdf],
    ] {
        let mut d = vec![30];
        d.extend(real);
        d.extend([12, 9]);
        assert!(
            dict(&d).is_none(),
            "malformed/unsupported real operand {d:?}"
        );
    }
}
#[test]
fn CIDFont_top_dictionary_must_start_with_ROS() {
    let mut a = font("Adobe", "Japan1", 6, 17518, 0);
    let mut p = a[2] as usize;
    index(&a, &mut p).unwrap();
    let ds = index(&a, &mut p).unwrap();
    let d = ds[0];
    let start = d.as_ptr() as usize - a.as_ptr() as usize;
    // Move the valid charset operator before ROS, retaining every offset and
    // dictionary value, and changing no object count or byte length.
    assert_eq!(&d[15..17], &[12, 30]);
    let mut reordered = d[17..23].to_vec();
    reordered.extend(&d[..17]);
    reordered.extend(&d[23..]);
    a[start..start + reordered.len()].copy_from_slice(&reordered);
    assert!(standard_cids(&a).is_none());
}
#[test]
fn real_operands_obey_the_same_48_entry_DICT_stack_limit() {
    let original = font("Adobe", "Japan1", 6, 17518, 0);
    let (_, _, fda, fds) = tables(&original);
    let private_offset = fds + 3;
    let mut private = Vec::new();
    for _ in 0..49 { private.extend([30, 0x0f]); }
    private.push(6);
    let mut bad = original[..fda].to_vec();
    bad.extend(idx(&[[num(private.len() as i32), num(private_offset as i32), vec![18]].concat()]));
    bad.extend([0, 0, 0]);
    assert_eq!(bad.len(), private_offset);
    bad.extend(private);
    assert!(standard_cids(&bad).is_none(), "49 real operands in a supported FD Private dictionary exceed the CFF1 stack limit");
}
