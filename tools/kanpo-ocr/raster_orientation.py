"""Candidate quarter-turn normalization from source-bound glyph quadrilaterals.

The glyph's right edge direction differs from the direction in which a line
is read. Upright vertical Japanese text therefore does not authorize rotating
the glyphs. Mixed, missing or contradictory geometry remains unresolved.
"""
import hashlib
import math
from PIL import Image
from source_gutters import ink_mask


def right_axis(quad, width, height):
    if (not isinstance(quad, list) or len(quad) != 4
            or any(not isinstance(p, list) or len(p) != 2
                   or any(type(v) not in (int, float) or not math.isfinite(v) for v in p) for p in quad)):
        return None
    points = [[x*width, (1-y)*height] for x,y in quad]
    if any(not (-.5 <= x <= width+.5 and -.5 <= y <= height+.5) for x,y in points):
        return None

    def direction(first, last):
        dx,dy=last[0]-first[0],last[1]-first[1]
        if abs(dx) == abs(dy):
            return None
        axis=(0 if dx>0 else 2) if abs(dx)>abs(dy) else (1 if dy>0 else 3)
        angle=math.degrees(math.atan2(dy,dx))%360
        residual=(angle-axis*90+180)%360-180
        return axis,residual
    top=direction(points[0],points[1]);bottom=direction(points[3],points[2])
    left=direction(points[0],points[3]);right=direction(points[1],points[2])
    if (None in (top,bottom,left,right) or top[0]!=bottom[0]
            or left[0]!=right[0] or left[0]!=(top[0]+1)%4):
        return None
    return {'axis':top[0],'edge_residual_degrees':[top[1],bottom[1],left[1],right[1]],
            'quad_source_pixels':points}


def observe_orientation(image, vision_page):
    if vision_page.get('revision') != 3 or vision_page.get('language_correction') is not False:
        raise ValueError('Expected uncorrected Vision revision 3 character geometry')
    if not isinstance(vision_page.get('observations'),list):
        raise ValueError('Missing Vision observations')
    width,height=image.size
    mask=ink_mask(image)
    evidence=[];unresolved=[];axes=set();supported_observations=set()
    for oi,observation in enumerate(vision_page['observations']):
        candidates=observation.get('candidates')
        if not isinstance(candidates,list) or not candidates:
            raise ValueError('Missing raw recognition candidate')
        candidate=candidates[0]
        if not isinstance(candidate.get('text'),str) or not isinstance(candidate.get('characters'),list):
            raise ValueError('Missing character range geometry')
        cursor=0;pieces=[]
        for ci,character in enumerate(candidate['characters']):
            text=character.get('text')
            if (not isinstance(text,str) or not text
                    or character.get('unicode_scalar_offset')!=cursor
                    or character.get('unicode_scalar_length')!=len(text)):
                raise ValueError('Character ranges differ from raw candidate text')
            pieces.append(text);cursor+=len(text)
            if not text.strip():
                continue
            item={'observation':oi,'character':ci,'unicode_scalar_offset':cursor-len(text)}
            basis=right_axis(character.get('quad'),width,height)
            if basis is None:
                unresolved.append(dict(item,reason='invalid_or_ambiguous_character_quadrilateral'))
                continue
            points=basis['quad_source_pixels']
            box=[max(0,math.floor(min(p[0] for p in points))),max(0,math.floor(min(p[1] for p in points))),
                 min(width,math.ceil(max(p[0] for p in points))),min(height,math.ceil(max(p[1] for p in points)))]
            if box[0]>=box[2] or box[1]>=box[3] or mask.crop(box).getbbox() is None:
                unresolved.append(dict(item,reason='no_recognition_copy_ink_in_character_rectangle'))
                continue
            evidence.append(dict(item,**basis,ink_sampling_box=box))
            axes.add(basis['axis']);supported_observations.add(oi)
        if ''.join(pieces)!=candidate['text']:
            raise ValueError('Character ranges do not reconstruct the complete candidate')
    # Do not choose the majority orientation of a mixed document. A second
    # independently detected region is required to support a global choice.
    selected=(next(iter(axes)) if len(axes)==1 and len(supported_observations)>=2
              and not any(u['reason']=='invalid_or_ambiguous_character_quadrilateral' for u in unresolved) else None)
    status=('unanimous_observed_glyph_right_axes' if selected is not None
            else 'mixed_glyph_axes' if len(axes)>1 else 'insufficient_orientation_evidence')
    return {'schema':1,'kind':'unclassified_raster_glyph_orientation',
            'recognition_RGBA_sha256':hashlib.sha256(image.convert('RGBA').tobytes()).hexdigest(),
            'canvas':{'pixel_width':width,'pixel_height':height},
            'selection_policy':'top1_character_geometry_unanimous_across_at_least_two_observations',
            'glyph_right_axis':selected,'quarter_turns_ccw':selected if selected is not None else 0,
            'status':status,'characters':evidence,'unresolved':unresolved,
            'coverage_verified':False}


def turn_matrix(width, height, turns):
    if (type(width) is not int or type(height) is not int or width<=0 or height<=0
            or type(turns) is not int or not 0<=turns<=3):
        raise ValueError('Expected a source canvas and a cardinal quarter turn')
    return ([1,0,0,1,0,0],[0,1,-1,0,width,0],
            [-1,0,0,-1,width,height],[0,-1,1,0,0,height])[turns][:]


def turn_image(image, turns):
    turn_matrix(image.width,image.height,turns)
    return (image.copy() if turns==0 else image.transpose(
        (Image.Transpose.ROTATE_90,Image.Transpose.ROTATE_180,Image.Transpose.ROTATE_270)[turns-1]))


def turn_frame(frame, turns):
    a,b,c,d,e,f=turn_matrix(frame['pixel_width'],frame['pixel_height'],turns)
    aa,bb,cc,dd,ee,ff=frame['recognition_to_image']
    matrix=[aa*a+cc*b,bb*a+dd*b,aa*c+cc*d,bb*c+dd*d,aa*e+cc*f+ee,bb*e+dd*f+ff]
    return {**frame,'pixel_width':frame['pixel_height'] if turns%2 else frame['pixel_width'],
            'pixel_height':frame['pixel_width'] if turns%2 else frame['pixel_height'],
            'recognition_to_image':[v if v!=0 else 0 for v in matrix]}


def observe_preparations(directories, output):
    """Capture local inputs and run one fixed character-geometry observation."""
    import json
    import subprocess
    import sys
    from pathlib import Path
    from io import BytesIO
    import source_gutters
    if sys.platform!='darwin':
        raise ValueError('This explicit character observer requires local macOS Vision')
    output=Path(output)
    if output.exists():
        raise FileExistsError(output)
    digest=lambda raw:hashlib.sha256(raw).hexdigest()
    inputs=[]
    for directory in map(Path,directories):
        prep_raw=(directory/'preparation.json').read_bytes();prep=json.loads(prep_raw)
        if prep.get('kind')!='source_derived_ocr_preparation' or prep.get('coverage_verified') is not False:
            raise ValueError('Expected an unclassified raster preparation')
        source=prep['source']
        checks={'source.pdf':source['document_sha256'],'source.png':source['source_PNG_sha256'],
                'source-binding.json':source['binding_sha256'],'source-observation.json':source['observation_sha256'],
                'source-receipt.json':source['source_receipt_sha256'],'source-placement.json':source['source_placement_sha256']}
        for name,sha in checks.items():
            if digest((directory/name).read_bytes())!=sha:
                raise ValueError('Original orientation input changed: '+name)
        png=(directory/'recognition-parent.png').read_bytes()
        image=Image.open(BytesIO(png)).convert('RGBA')
        if (digest(png)!=prep['recognition_parent_sha256']
                or [image.width,image.height]!=[prep['canvas']['pixel_width'],prep['canvas']['pixel_height']]):
            raise ValueError('Recognition copy differs from its declared source canvas')
        inputs.append({'source':source,'prepared':str(directory.resolve()),
                       'preparation_sha256':digest(prep_raw),'recognition_parent_sha256':digest(png),
                       'PNG':png})
    output.mkdir();runtime=output/'runtime';runtime.mkdir();captured=output/'inputs';captured.mkdir()
    source_bytes=Path(__file__).with_name('recognize_characters.swift').read_bytes()
    (runtime/'recognize_characters.swift').write_bytes(source_bytes);(runtime/'recognize_characters.swift').chmod(0o400)
    observer_bytes=Path(__file__).read_bytes()
    (runtime/'raster_orientation.py').write_bytes(observer_bytes);(runtime/'raster_orientation.py').chmod(0o400)
    gutter_bytes=Path(source_gutters.__file__).read_bytes()
    (runtime/'source_gutters.py').write_bytes(gutter_bytes);(runtime/'source_gutters.py').chmod(0o400)
    manifest=[]
    for i,record in enumerate(inputs):
        path=captured/f'parent-{i:04}.png';path.write_bytes(record['PNG']);path.chmod(0o400)
        manifest.append({k:v for k,v in record.items() if k!='PNG'}|{'image':str(path.resolve())})
    def write(name,value):
        (output/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
    write('manifest.json',{'schema':1,'kind':'source_bound_character_geometry_inputs','rows':manifest,
                           'swift_source_sha256':digest(source_bytes),'observer_sha256':digest(observer_bytes),
                           'ink_helper_sha256':digest(gutter_bytes),'pillow':Image.__version__,'python':sys.version,
                           'coverage_verified':False})
    try:
        with (runtime/'compile.stdout').open('xb') as stdout,(runtime/'compile.stderr').open('xb') as stderr:
            build=subprocess.run(['xcrun','swiftc','-O',str(runtime/'recognize_characters.swift'),'-o',str(runtime/'recognize')],stdout=stdout,stderr=stderr)
        if build.returncode:
            raise RuntimeError('Character observer compilation failed; see runtime/compile.stderr')
        (runtime/'recognize').chmod(0o500)
        with (output/'vision-raw.json').open('xb') as stdout,(output/'vision-stderr.txt').open('xb') as stderr:
            process=subprocess.run([str((runtime/'recognize').resolve()),*[r['image'] for r in manifest]],stdout=stdout,stderr=stderr)
        if process.returncode:
            raise RuntimeError('Character observer failed; see vision-stderr.txt')
        raw=(output/'vision-raw.json').read_bytes();vision=json.loads(raw)
        if len(vision['outputs'])!=len(manifest):
            raise ValueError('Character observer changed the captured input inventory')
        rows=[]
        for i,(record,page) in enumerate(zip(manifest,vision['outputs'])):
            try:
                if page.get('path')!=record['image']:
                    raise ValueError('Character geometry belongs to another captured input')
                if 'error' in page:
                    raise ValueError(page['error'])
                png=Path(record['image']).read_bytes()
                if digest(png)!=record['recognition_parent_sha256']:
                    raise ValueError('Actual observed PNG changed')
                observation=observe_orientation(Image.open(BytesIO(png)).convert('RGBA'),page)
                report={'schema':1,'kind':'source_derived_raster_orientation','source':record['source'],
                        'recognition_parent_sha256':record['recognition_parent_sha256'],
                        'vision':{'page':page,'os':vision['os'],'raw_sha256':digest(raw),
                                  'manifest_sha256':digest((output/'manifest.json').read_bytes()),
                                  'source_sha256':digest(source_bytes),'binary_sha256':digest((runtime/'recognize').read_bytes()),
                                  'runtime_directory':str(runtime.resolve())},'observation':observation}
                name=f'orientation-{i:04}.json';write(name,report)
                rows.append({**record,'orientation':name,'orientation_sha256':digest((output/name).read_bytes()),
                             'status':observation['status'],'quarter_turns_ccw':observation['quarter_turns_ccw']})
            except Exception as error:
                rows.append({**record,'status':'failed','error':str(error)})
        index={'schema':1,'kind':'unclassified_source_glyph_orientations','rows':rows,'coverage_verified':False}
        write('index.json',index)
        write('terminal.json',{'status':'partial' if any(r['status']=='failed' for r in rows) else 'complete',
                               'recognition_runs':len(manifest),'canonical_integration':False})
        return index
    except Exception as error:
        write('terminal.json',{'status':'failed','error':str(error),'recognition_retry':False,'canonical_integration':False})
        raise


def main():
    import argparse
    import json
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prepared',nargs='+')
    parser.add_argument('--output',required=True)
    args=parser.parse_args()
    result=observe_preparations(args.prepared,args.output)
    print(json.dumps({'images':len(result['rows']),'failed':sum(r['status']=='failed' for r in result['rows']),
                      'coverage_verified':False}))
    return 2 if any(r['status']=='failed' for r in result['rows']) else 0


if __name__=='__main__':
    raise SystemExit(main())
