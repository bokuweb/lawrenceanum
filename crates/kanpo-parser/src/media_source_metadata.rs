//! Preserve declared image dictionaries and clips without resolving them into
//! guessed pixels or treating graphics-state names as visibility evidence.
use kanpo_pdf::{ImageClipOrigin, ImageClipRule, ImageDraw, Object};
use serde_json::{json, Value};
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|n| format!("{n:02x}")).collect()
}
fn number(n: f64) -> Value {
    if n.is_finite() {
        json!(n)
    } else {
        json!({"$f64_bits":format!("{:016x}",n.to_bits())})
    }
}
fn numbers(ns: &[f64]) -> Value {
    Value::Array(ns.iter().map(|&n| number(n)).collect())
}
fn object(value: &Object) -> Value {
    match value {
        Object::Null => json!({"type":"null"}),
        Object::Bool(v) => json!({"type":"bool","value":v}),
        Object::Int(v) => json!({"type":"int","value":v}),
        Object::Real(v) => json!({"type":"real","value":number(*v)}),
        Object::Name(v) => json!({"type":"name","hex":hex(v)}),
        Object::String(v) => json!({"type":"string","hex":hex(v)}),
        Object::Ref(v) => json!({"type":"ref","object":v.num,"generation":v.gen}),
        Object::Array(v) => {
            json!({"type":"array","values":v.iter().map(object).collect::<Vec<_>>()})
        }
        Object::Dict(v) => dict(v),
        Object::Stream(v) => {
            json!({"type":"stream","dictionary":dict(&v.dict),"encoded_bytes":v.data.len()})
        }
    }
}
fn dict(value: &kanpo_pdf::Dict) -> Value {
    json!({"type":"dict","entries":value.iter().map(|(k,v)|json!([hex(k),object(v)])).collect::<Vec<_>>()})
}
pub(super) fn describe(draw: &ImageDraw) -> Value {
    json!({
        "resource_name_hex":hex(&draw.resource_name),
        "object_ref":draw.object_ref.map(|r|[r.num as u64,r.gen as u64]),
        "form_path":draw.form_path.iter().map(|r|[r.num as u64,r.gen as u64]).collect::<Vec<_>>(),
        "pixel_width":draw.pixel_width,"pixel_height":draw.pixel_height,
        "image_to_user":numbers(&draw.image_to_user),
        "source_geometry_complete":draw.source_geometry_complete,
        "graphics_state_names_hex":draw.graphics_states.iter().map(|n|hex(n)).collect::<Vec<_>>(),
        "declared_dictionary":dict(draw.declared_dictionary()),
        "clips":draw.clips.iter().map(clip).collect::<Vec<_>>()
    })
}

fn commands(items:&[kanpo_pdf::PathCommand])->Value {
    Value::Array(items.iter().map(|c|json!({"operator_hex":hex(&c.operator),"operands":c.operands.iter().map(object).collect::<Vec<_>>(),"matrix":numbers(&c.matrix),"matrices":c.matrices.iter().map(|m|numbers(m)).collect::<Vec<_>>()})).collect())
}
fn clip(clip:&kanpo_pdf::ImageSourceClip)->Value {
    json!({"commands":commands(&clip.commands),
            "origin":match clip.origin{ImageClipOrigin::Path=>"path",ImageClipOrigin::FormBox=>"form_bbox"},
            "rule":match clip.rule{ImageClipRule::NonZero=>"nonzero",ImageClipRule::EvenOdd=>"evenodd"},
            "geometry_complete":clip.geometry_complete,
            "form_bbox":clip.form_box.map(|b|numbers(&b)),
            "page_quad":clip.page_quad.map(|q|q.map(|p|numbers(&p))),
            "subpaths":clip.subpaths.iter().map(|s|json!({"closed":s.closed,"curved":s.curved,"rect":s.rect,
                "points":s.points.iter().map(|p|numbers(p)).collect::<Vec<_>>()})).collect::<Vec<_>>()
        })
}
pub(super) fn path(draw:&kanpo_pdf::PathDraw)->Value {
    json!({"path":draw.path,"operator_hex":hex(&draw.operator),"commands":commands(&draw.commands),
        "matrix":numbers(&draw.matrix),"matrices":draw.matrices.iter().map(|m|numbers(m)).collect::<Vec<_>>(),"form_path":draw.form_path.iter().map(|r|[r.num as u64,r.gen as u64]).collect::<Vec<_>>(),
        "clips":draw.clips.iter().map(clip).collect::<Vec<_>>(),
        "state":draw.state.iter().map(|s|json!({"operator_hex":hex(&s.operator),"operands":s.operands.iter().map(object).collect::<Vec<_>>(),"resource":s.resource.as_ref().map(object)})).collect::<Vec<_>>()})
}
