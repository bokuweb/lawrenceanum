use crate::source_images::tests::{check,objects,pdf,stream};

#[test]
fn curves_dash_paint_rule_and_clip_timing_keep_exact_native_operators() {
    let a=check(&pdf(objects(b"[3 2] 1 d 2 J 1 j 7 M .3 G 2 w 0 0 m 1 2 3 4 5 6 c 7 8 9 10 v 11 12 13 14 y W* B* 20 20 5 5 re f")));
    let paths=a[0].paths();
    assert_eq!(paths.len(),2);
    assert_eq!(paths[0].operator,b"B*");
    assert!(paths[0].clips.is_empty(),"new clipping begins AFTER painting its path");
    assert_eq!(paths[0].commands.iter().map(|c|c.operator.as_slice()).collect::<Vec<_>>(),[b"m".as_slice(),b"c",b"v",b"y"]);
    assert_eq!(paths[0].commands[1].operands.iter().map(|v|v.as_f64().unwrap()).collect::<Vec<_>>(),[1.,2.,3.,4.,5.,6.]);
    assert_eq!(paths[0].state.iter().map(|s|s.operator.as_slice()).collect::<Vec<_>>(),[b"d".as_slice(),b"J",b"j",b"M",b"G",b"w"]);
    assert_eq!(paths[1].clips.len(),1);
    assert_eq!(paths[1].clips[0].rule,crate::source_images::ClipRule::EvenOdd);
    assert_eq!(paths[1].clips[0].commands.len(),4);
}
#[test]
fn state_stack_form_resources_and_midpath_transforms_remain_in_their_scope() {
    let mut o=objects(b"[3 2] 1 d q [9] 0 d 0 0 m 2 0 0 3 4 5 cm 5 6 l S Q /F Do 0 0 3 4 re S");
    o[4]=stream(b"/A gs [8 1] 2 d 0 0 1 1 re S","/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Matrix [2 0 0 3 10 20] /Resources << /ExtGState << /A << /LW 4 /D [[7 6] 5] /CA .5 >> >> >>");
    let a=check(&pdf(o));let p=a[0].paths();assert_eq!(p.len(),3);
    assert_eq!(p[0].commands[0].matrix,[1.,0.,0.,1.,0.,0.]);
    assert_eq!(p[0].commands[1].matrix,[2.,0.,0.,3.,4.,5.]);
    assert_eq!(p[0].commands[1].matrices,vec![[2.,0.,0.,3.,4.,5.]]);
    assert_eq!(p[1].form_path.len(),1);assert_eq!(p[1].matrix,[2.,0.,0.,3.,10.,20.]);
    let gs=p[1].state.iter().find(|s|s.operator==b"gs").unwrap();
    assert_eq!(gs.resource.as_ref().unwrap().as_dict().unwrap().get(&b"LW"[..]).unwrap().as_f64(),Some(4.));
    assert_eq!(p[2].state.len(),1,"neither q/Q nor Form state leaks");
    assert_eq!(p[2].matrix,[1.,0.,0.,1.,0.,0.]);
}
