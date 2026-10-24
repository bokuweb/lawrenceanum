use crate::lines::{HRule, PageRules, VRule};
use crate::{analyze_item, trace::Role, PageInput};
fn check(n: usize) {
    let mut x = String::from("<html><body><doc><page width=\"595\" height=\"842\">");
    for (y, y1) in [(70., 410.), (432., 780.)] {
        for k in 0..8 {
            let a = 200. + k as f32 * 42.;
            x.push_str(&format!("<word xMin=\"{a}\" xMax=\"{}\" yMin=\"{y}\" yMax=\"{y1}\">比較する規定の本文{k}を記載する。</word>",a+8.));
        }
    }
    for k in 0..n {
        let a = 120. - k as f32 * 12.;
        let (y, y1) = if k % 2 == 0 {
            (90., 710.)
        } else {
            (130., 750.)
        };
        x.push_str(&format!("<word xMin=\"{a}\" xMax=\"{}\" yMin=\"{y}\" yMax=\"{y1}\">独立した欄外の本文をここに記載する。</word>",a+8.));
    }
    x.push_str("</page></doc></body></html>");
    let mut rules = PageRules::default();
    rules.vertical.push(VRule {
        x: 160.,
        y0: 70.,
        y1: 790.,
    });
    rules.horizontal.push(HRule {
        y: 421.,
        x0: 160.,
        x1: 550.,
    });
    let a = analyze_item(&[PageInput {
        xhtml: x,
        rules: Some(rules),
    }]);
    for id in 16..16 + n {
        let roles: Vec<_> = a.pages[0]
            .regions
            .iter()
            .filter(|r| {
                r.lines.iter().any(|l| {
                    l.words.contains(&id)
                        || l.table
                            .as_ref()
                            .is_some_and(|t| t.cells.iter().any(|c| c.words.contains(&id)))
                })
            })
            .map(|r| r.role)
            .collect();
        assert_eq!(roles.len(), 1, "outside source word{id} has one owner");
        assert!(
            !matches!(roles[0], Role::Before | Role::After),
            "outside word{id} for outside-column count{n} became a comparison field. text={}",
            a.text
        );
    }
}
macro_rules! c {
    ($name:ident,$n:literal) => {
        #[test]
        fn $name() {
            check($n)
        }
    };
}
c!(one, 1);
c!(two, 2);
c!(three, 3);
c!(four, 4);
c!(five, 5);
c!(six, 6);
c!(seven, 7);
