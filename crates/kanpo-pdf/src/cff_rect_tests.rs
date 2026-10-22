use super::*;
fn idx(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = (items.len() as u16).to_be_bytes().to_vec();
    if items.is_empty() {
        return out;
    }
    out.push(2);
    let mut n = 1u16;
    out.extend(n.to_be_bytes());
    for item in items {
        n += item.len() as u16;
        out.extend(n.to_be_bytes());
    }
    for item in items {
        out.extend(item);
    }
    out
}
fn num(n: i32) -> Vec<u8> {
    let mut s = vec![29];
    s.extend(n.to_be_bytes());
    s
}
fn op(args: &[i32], code: &[u8]) -> Vec<u8> {
    args.iter()
        .flat_map(|&n| num(n))
        .chain(code.iter().copied())
        .collect()
}
fn csnum(n: i16) -> Vec<u8> {
    let mut b = vec![28];
    b.extend(n.to_be_bytes());
    b
}
pub(crate) fn program(points: &[[i16; 2]]) -> Vec<u8> {
    let mut b = vec![];
    let mut p = [0i16, 0];
    for (i, &q) in points.iter().enumerate() {
        b.extend(csnum(q[0] - p[0]));
        b.extend(csnum(q[1] - p[1]));
        b.push(if i == 0 { 21 } else { 5 });
        p = q;
    }
    b.push(14);
    b
}
pub(crate) fn font(
    cid: u16,
    cs: Vec<u8>,
    name: &str,
    registry: &str,
    ordering: &str,
    top_extra: &[u8],
    fd_extra: &[u8],
) -> Vec<u8> {
    let names = idx(&[name.as_bytes().to_vec()]);
    let strings = idx(&[registry.as_bytes().to_vec(), ordering.as_bytes().to_vec()]);
    let global = idx(&[]);
    let charset = vec![0, (cid >> 8) as u8, cid as u8];
    let select = vec![0, 0, 0];
    let chars = idx(&[vec![14], cs]);
    let mut offsets = [0i32; 5];
    let mut result = vec![];
    for _ in 0..3 {
        let mut top = op(&[391, 392, 0], &[12, 30]);
        top.extend(op(&[offsets[0]], &[15]));
        top.extend(op(&[offsets[1]], &[17]));
        top.extend(op(&[offsets[2]], &[12, 36]));
        top.extend(op(&[offsets[3]], &[12, 37]));
        top.extend(top_extra);
        let top = idx(&[top]);
        let mut fd = op(&[0, offsets[4]], &[18]);
        fd.extend(fd_extra);
        let fd = idx(&[fd]);
        let base = 4 + names.len() + top.len() + strings.len() + global.len();
        offsets = [
            base as i32,
            (base + charset.len()) as i32,
            (base + charset.len() + chars.len()) as i32,
            (base + charset.len() + chars.len() + fd.len()) as i32,
            (base + charset.len() + chars.len() + fd.len() + select.len()) as i32,
        ];
        result = vec![1, 0, 4, 4];
        for part in [
            &names, &top, &strings, &global, &charset, &chars, &fd, &select,
        ] {
            result.extend(part);
        }
    }
    result
}
fn rectangle() -> Vec<u8> {
    program(&[
        [1002, 414],
        [-2, 414],
        [-2, 352],
        [999, 352],
        [1002, 352],
        [1002, 414],
    ])
}
#[test]
fn geometry_comes_from_the_charset_selected_outline_not_names_CIDs_or_Unicode() {
    for (cid, name, reg, ord) in [
        (73, "Opaque", "Any", "Other"),
        (43021, "OtherFace", "Different", "Random"),
    ] {
        let got = rectangles(&font(cid, rectangle(), name, reg, ord, &[], &[])).unwrap();
        assert_eq!(got.get(&cid), Some(&[-2., 352., 1002., 414.]));
        assert!(!got.contains_key(&0));
    }
}
#[test]
fn the_original_embedded_outline_has_the_independently_traced_rectangle() {
    let bytes = include_bytes!("../tests/data/native-ruling-original.cff");
    let got = rectangles(bytes).unwrap();
    assert_eq!(got.get(&23073), Some(&[-2., 352., 1002., 414.]));
    assert_eq!(got.get(&23075), Some(&[468., -122., 530., 882.]));
}
#[test]
fn curves_triangles_holes_and_retracing_are_not_filled_rectangles() {
    let mut curved = rectangle();
    curved[6] = 8;
    let mut holes = rectangle();
    holes.pop();
    holes.extend(program(&[[0, 0], [1, 0], [1, 1], [0, 1]]));
    let zig = program(&[[0, 0], [100, 0], [0, 0], [100, 0], [100, 20], [0, 20]]);
    for cs in [
        curved,
        holes,
        program(&[[0, 0], [100, 0], [0, 20]]),
        zig,
        vec![14],
        vec![139, 139, 21, 14],
    ] {
        let got = rectangles(&font(81, cs, "Test", "Any", "Any", &[], &[])).unwrap();
        assert!(!got.contains_key(&81));
    }
}
#[test]
fn unsupported_font_geometry_and_painting_never_obtain_authority() {
    let explicit_matrix = op(&[1, 0, 0, 1, 0, 0], &[12, 7]);
    for (top, fd) in [
        (op(&[2], &[12, 5]), vec![]),
        (op(&[1], &[12, 6]), vec![]),
        (explicit_matrix.clone(), vec![]),
        (vec![], explicit_matrix),
        (vec![], op(&[2], &[12, 5])),
    ] {
        assert!(rectangles(&font(91, rectangle(), "Test", "Any", "Any", &top, &fd)).is_none());
    }
}
#[test]
fn malformed_or_duplicate_routes_and_truncated_programs_are_not_rectangles() {
    let good = font(81, rectangle(), "Test", "Any", "Any", &[], &[]);
    for end in [0, 3, good.len() - 1] {
        assert!(rectangles(&good[..end]).is_none());
    }
    let mut dup = font(0, rectangle(), "Test", "Any", "Any", &[], &[]);
    assert!(rectangles(&dup).is_none());
    dup[0] = 2;
    assert!(rectangles(&dup).is_none());
    let mut cs = rectangle();
    cs.pop();
    let got = rectangles(&font(81, cs, "Test", "Any", "Any", &[], &[])).unwrap();
    assert!(!got.contains_key(&81));
    let mut cs = rectangle();
    cs.push(14);
    let got = rectangles(&font(81, cs, "Test", "Any", "Any", &[], &[])).unwrap();
    assert!(!got.contains_key(&81));
}
