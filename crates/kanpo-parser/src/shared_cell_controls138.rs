use super::*;
use crate::lines::{HRule,VRule};
fn fixture(boundary:f32, shared:bool)->(Vec<Col>,PageRules) {
    let rules=PageRules {
        enclosures:vec![],short_fraction_bars:vec![],
        vertical:[74.,boundary,310.9,333.5].into_iter().map(|x|VRule{x,y0:69.5,y1:412.5}).collect(),
        horizontal:vec![HRule{y:69.5,x0:74.,x1:333.5},HRule{y:241.,x0:if shared{boundary}else{74.},x1:333.5},HRule{y:412.5,x0:74.,x1:333.5}],
    };
    let body=(boundary+310.9-8.)/2.;
    let col=|id,x,y,y1,text:&str|Col{id,x,x1:x+8.,y,y1,text:text.into()};
    let cols=vec![col(0,body,75.,139.,"arbitrary left body"),col(1,body,250.,314.,"unrelated right body"),col(2,90.,75.,123.,"prefix"),col(3,90.,if shared{131.}else{250.},398.,"suffix")];
    (cols,rules)
}
fn unit(cols:Vec<Col>,rules:Option<&PageRules>)->Unit {
    let grids=rules.map(|r|crate::grid::find_grids(r,595.,842.)).unwrap_or_default();
    let mut layout=ShinkyuLayout::split(cols,315.,241.);layout.clip_to_frame(&grids,315.,241.);layout.into_unit(&grids,rules)
}
fn ids(parts:&[Vec<Line>])->Vec<usize>{parts.iter().flatten().flat_map(|l|l.words.clone()).collect()}
#[test]
fn wide_shared_cells_keep_one_source_owner_without_body_text_patterns() {
    for width in [111.25,138.,170.,250.,290.] {
        let(cols,rules)=fixture(width,true);let Unit::Shinkyu{after,before,..}=unit(cols,Some(&rules))else{panic!()};
        assert!(!ids(&before).contains(&2)&&!ids(&before).contains(&3));
        assert!(after.iter().flatten().any(|l|l.words.contains(&2)&&l.words.contains(&3)));
        assert!(ids(&before).contains(&1));assert!(ids(&after).contains(&0));
    }
}
#[test]
fn a_real_separator_keeps_the_two_cells_distinct_even_when_the_left_cell_is_wide() {
    for width in [138.,170.,250.] {
        let(cols,rules)=fixture(width,false);let Unit::Shinkyu{after,before,..}=unit(cols,Some(&rules))else{panic!()};
        assert!(ids(&after).contains(&2));assert!(!ids(&after).contains(&3));
        assert!(ids(&before).contains(&3));assert!(!ids(&before).contains(&2));
    }
}
#[test]
fn missing_native_grid_does_not_infer_a_shared_cell_from_aligned_text() {
    let(cols,_)=fixture(170.,true);let Unit::Shinkyu{after,before,..}=unit(cols,None)else{panic!()};
    assert!(ids(&after).contains(&2));assert!(!ids(&after).contains(&3));assert!(ids(&before).contains(&3));
}
