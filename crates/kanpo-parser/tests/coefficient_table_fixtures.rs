use kanpo_parser::trace::{Table,WordStatus};
const PDF:&[u8]=include_bytes!("archive/pages/20260724g00165/20260724g001650694.pdf");
fn actual()->kanpo_parser::Analysis { kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap()) }
fn tables(a:&kanpo_parser::Analysis)->Vec<&Table>{a.pages[0].regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).collect()}
const NEW_18: [&str;18] = [
    "２級地（旧７級地であつた地域）　1.029",
    "２級地（旧無級地であつた地域）　1.027",
    "３級地（旧２級地であつた地域）　1.036",
    "３級地（旧３級地であつた地域）　1.035",
    "３級地（旧４級地であつた地域）　1.033",
    "３級地（旧５級地であつた地域）　1.033",
    "３級地（旧６級地であつた地域）　1.029",
    "３級地（旧無級地であつた地域）　1.022",
    "４級地（旧４級地であつた地域）　1.027",
    "４級地（旧５級地であつた地域）　1.022",
    "４級地（旧６級地であつた地域）　1.022",
    "４級地（旧７級地であつた地域）　1.022",
    "４級地（旧無級地であつた地域）　1.017",
    "５級地（旧６級地であつた地域）　1.010",
    "５級地（旧７級地であつた地域）　1.010",
    "５級地（旧無級地であつた地域）　1.010",
    "無級地（旧７級地であつた地域）　1.002",
    "無級地（旧無級地であつた地域）　0.999",
];

const OLD_18: [&str;18] = [
    "２級地（旧７級地であつた地域）　1.018",
    "２級地（旧無級地であつた地域）　1.009",
    "３級地（旧２級地であつた地域）　1.038",
    "３級地（旧３級地であつた地域）　1.036",
    "３級地（旧４級地であつた地域）　1.031",
    "３級地（旧５級地であつた地域）　1.029",
    "３級地（旧６級地であつた地域）　1.026",
    "３級地（旧無級地であつた地域）　1.009",
    "４級地（旧４級地であつた地域）　1.029",
    "４級地（旧５級地であつた地域）　1.024",
    "４級地（旧６級地であつた地域）　1.018",
    "４級地（旧７級地であつた地域）　1.014",
    "４級地（旧無級地であつた地域）　1.009",
    "５級地（旧６級地であつた地域）　1.012",
    "５級地（旧７級地であつた地域）　1.008",
    "５級地（旧無級地であつた地域）　1.004",
    "無級地（旧７級地であつた地域）　1.004",
    "無級地（旧無級地であつた地域）　0.999",
];

const OLD_IDS:[usize;72] = [5, 8, 9, 10, 13, 14, 18, 19, 21, 22, 24, 25, 27, 28, 30, 31, 32, 33, 44, 47, 48, 49, 52, 53, 57, 58, 60, 61, 63, 64, 66, 67, 69, 70, 71, 72, 73, 79, 81, 83, 87, 89, 96, 98, 102, 104, 108, 110, 114, 116, 120, 122, 124, 126, 192, 193, 194, 198, 199, 203, 204, 208, 209, 211, 212, 214, 215, 217, 218, 219, 220, 221];

#[test]fn all_eighteen_old_coefficients_keep_literal_pairs_in_right_table_and_page_text(){
 let a=actual();let ts=tables(&a);assert_eq!(ts.len(),2);let t=ts[1];assert_eq!((t.rows,t.cols),(36,3));
 for (i,want) in OLD_18.iter().enumerate(){let c=t.cells.iter().find(|c|c.row==i&&c.col==2).unwrap();assert_eq!(&c.text,want);}
 let lines:Vec<_>=a.pages[0].text.lines().collect();assert_eq!(lines.len(),72);
 for (i,want) in OLD_18.iter().enumerate(){assert_eq!(lines[36+i].trim_start_matches('　'),*want);}
 assert_eq!(a.text,a.pages[0].text);
}
#[test]fn all_seventy_two_original_words_are_used_once_in_the_right_physical_cell(){
 let a=actual();let p=&a.pages[0];let ts=tables(&a);let t=ts[1];assert_eq!([t.x0,t.y0,t.x1,t.y1],[442.75,46.,774.75,549.]);
 let mut ids:Vec<_>=t.cells.iter().filter(|c|c.row<18).flat_map(|c|c.words.iter().copied()).collect();ids.sort_unstable();assert_eq!(ids,OLD_IDS);
 for c in t.cells.iter().filter(|c|c.row<18){assert_eq!([c.x0,c.y0,c.x1,c.y1],[554.75,46.,774.75,274.464]);}
 for id in OLD_IDS { let w=p.words.iter().find(|w|w.id==id).unwrap();assert_eq!(w.status,WordStatus::Used);assert!(w.x0>=554.75&&w.x1<=774.75&&w.y0>=46.&&w.y1<=274.464);let owners:usize=p.regions.iter().flat_map(|r|&r.lines).map(|l|l.words.iter().filter(|&&n|n==id).count()+l.table.iter().flat_map(|t|&t.cells).flat_map(|c|&c.words).filter(|&&n|n==id).count()).sum();assert_eq!(owners,1,"id={id}"); }
}
#[test]fn neighboring_new_side_and_later_school_six_coefficients_do_not_merge_into_the_old_upper_table(){
 let a=actual();let ts=tables(&a);
 for (i,want) in NEW_18.iter().enumerate(){assert_eq!(&ts[0].cells.iter().find(|c|c.row==i&&c.col==2).unwrap().text,want);}
 let new_tail=["１級地（旧１級地であつた地域）　1.147","２級地（旧２級地であつた地域）　1.118","２級地（旧３級地であつた地域）　1.118","２級地（旧４級地であつた地域）　1.118","２級地（旧５級地であつた地域）　1.110","２級地（旧６級地であつた地域）　1.099"];
 let old_tail=["１級地（旧１級地であつた地域）　1.147","２級地（旧２級地であつた地域）　1.119","２級地（旧３級地であつた地域）　1.109","２級地（旧４級地であつた地域）　1.102","２級地（旧５級地であつた地域）　1.102","２級地（旧６級地であつた地域）　1.075"];
 let lines:Vec<_>=a.text.lines().collect();for i in 0..6{assert_eq!(lines[30+i].trim_start_matches('　'),new_tail[i]);assert_eq!(lines[66+i].trim_start_matches('　'),old_tail[i]);}
 assert_eq!(lines[36].trim_start_matches('　'),OLD_18[0]);assert!(lines[54].trim_start_matches('　').starts_with("寒冷補正"));
 for (i,want) in old_tail.iter().enumerate(){assert_eq!(&ts[1].cells.iter().find(|c|c.row==30+i&&c.col==2).unwrap().text,want);}
}
#[test]fn all_native_glyphs_in_old_upper_rectangle_are_present_and_not_a_zero_layer_exclusion(){
 let ps=kanpo_pdf::extract_pages(PDF).unwrap();let r=[553.,49.,780.,270.];let gs:Vec<_>=ps[0].glyphs.iter().filter(|g|g.bbox[0]<r[2]&&g.bbox[2]>r[0]&&g.bbox[1]<r[3]&&g.bbox[3]>r[1]).collect();assert_eq!(gs.len(),411);
}
