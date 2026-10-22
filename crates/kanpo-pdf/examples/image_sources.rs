use kanpo_pdf::{ImageClipOrigin, ImageClipRule, Object};
use serde_json::{json, Value};
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn number(n: f64) -> Value {
    if n.is_finite() {
        json!(n)
    } else {
        json!({"$f64_bits":format!("{:016x}",n.to_bits())})
    }
}
fn matrix(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|v| number(*v)).collect())
}
fn object(v: &Object) -> Value {
    match v {
        Object::Null => json!({"type":"null"}),
        Object::Bool(b) => json!({"type":"bool","value":b}),
        Object::Int(i) => json!({"type":"int","value":i}),
        Object::Real(r) => json!({"type":"real","value":number(*r)}),
        Object::Name(n) => json!({"type":"name","hex":hex(n)}),
        Object::String(n) => json!({"type":"string","hex":hex(n)}),
        Object::Ref(r) => json!({"type":"ref","object":r.num,"generation":r.gen}),
        Object::Array(a) => {
            json!({"type":"array","values":a.iter().map(object).collect::<Vec<_>>()})
        }
        Object::Dict(d) => dict(d),
        Object::Stream(s) => {
            json!({"type":"stream","dictionary":dict(&s.dict),"encoded_bytes":s.data.len()})
        }
    }
}
fn dict(d: &kanpo_pdf::Dict) -> Value {
    json!({"type":"dict","entries":d.iter().map(|(k,v)|json!([hex(k),object(v)])).collect::<Vec<_>>()})
}
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: image_sources <original.pdf>");
    let input = std::fs::read(&path).unwrap();
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(&input).unwrap();
    let pages=sources.iter().enumerate().map(|(pi,p)|{
 let draws=p.images().iter().map(|d|{
 let clips=d.clips.iter().map(|c|json!({"origin":match c.origin{ImageClipOrigin::Path=>"path",ImageClipOrigin::FormBox=>"form_bbox"},"rule":match c.rule{ImageClipRule::NonZero=>"nonzero",ImageClipRule::EvenOdd=>"evenodd"},"geometry_complete":c.geometry_complete,"form_bbox":c.form_box.map(|b|matrix(&b)),"page_quad":c.page_quad.map(|q|q.map(|v|matrix(&v))),"subpaths":c.subpaths.iter().map(|s|json!({"closed":s.closed,"curved":s.curved,"rect":s.rect,"points":s.points.iter().map(|v|matrix(v)).collect::<Vec<_>>()})).collect::<Vec<_>>() })).collect::<Vec<_>>();
 json!({"draw":d.sequence,"resource_name_hex":hex(&d.resource_name),"object_ref":d.object_ref.map(|r|[r.num as u64,r.gen as u64]),"form_path":d.form_path.iter().map(|r|[r.num as u64,r.gen as u64]).collect::<Vec<_>>(),"pixel_width":d.pixel_width,"pixel_height":d.pixel_height,"image_to_user":matrix(&d.image_to_user),"source_geometry_complete":d.source_geometry_complete,"encoded_sha256":hex(&d.encoded_sha256()),"encoded_bytes":d.encoded_bytes().len(),"declared_dictionary":dict(d.declared_dictionary()),"clips":clips,"graphics_state_names_hex":d.graphics_states.iter().map(|n|hex(n)).collect::<Vec<_>>()})
 }).collect::<Vec<_>>();
 json!({"page":pi,"document_sha256":hex(&p.document_sha256()),"media_box":matrix(&p.media_box()),"crop_box":matrix(&p.crop_box()),"rotate":p.page().rotate,"visibility_verified":p.visibility_verified(),"gaps":p.capture_gaps().iter().map(|g|json!({"kind":format!("{:?}",g.kind),"resource_hex":hex(&g.resource_name),"form_path":g.form_path.iter().map(|r|[r.num as u64,r.gen as u64]).collect::<Vec<_>>()})).collect::<Vec<_>>(),"images":draws})
 }).collect::<Vec<_>>();
    println!(
        "{}",
        json!({"schema":1,"source":"native_xobject_snapshot","pages":pages})
    );
}
