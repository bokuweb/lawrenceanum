"""Source-bound editable raster words and candidate cell geometry.

Raw observations remain available, including context-only and conflicting
recognitions. Exact source ink, rather than spelling or confidence, identifies
aliases. Cell candidates are geometry; their semantics and text remain unverified.
"""
import argparse
import copy
import hashlib
import json
import math
import re
from io import BytesIO
from pathlib import Path
from PIL import Image
from source_gutters import ink_mask


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def contains(a,b):
    return a[0]<=b[0] and a[1]<=b[1] and b[2]<=a[2] and b[3]<=a[3]


def overlap(a,b):
    return max(a[0],b[0])<min(a[2],b[2]) and max(a[1],b[1])<min(a[3],b[3])


def pixel_box(box,width,height):
    return (isinstance(box,list) and len(box)==4 and all(type(v) is int for v in box)
            and 0<=box[0]<box[2]<=width and 0<=box[1]<box[3]<=height)


def local(directory,name):
    if not isinstance(name,str) or Path(name).name!=name or name in ('','.','..'):
        raise ValueError('Expected a local raster layout member')
    return directory/name


def layout_box(box,width,height,turn):
    x0,y0,x1,y1=box
    points=[]
    for x,y in ((x0,y0),(x1,y0),(x1,y1),(x0,y1)):
        points.append(((x,y),(y,width-x),(width-x,height-y),(height-y,x))[turn])
    return [min(p[0] for p in points),min(p[1] for p in points),
            max(p[0] for p in points),max(p[1] for p in points)]


def source_box(tile,bbox,turn):
    x,y,r,b=tile['recognition_roi'];w,h=r-x,b-y
    expected=([1,0,0,1,x,y],[0,1,-1,0,r,y],[-1,0,0,-1,r,b],[0,-1,1,0,x,b])[turn]
    if tile.get('recognition_to_source',expected if turn==0 else None)!=expected:
        raise ValueError('Recognition coordinates differ from the original source crop')
    u0,v0,u1,v1=[bbox[k] for k in ('x0','y0','x1','y1')]
    cw,ch=(h,w) if turn%2 else (w,h)
    if (any(type(v) is not int for v in (u0,v0,u1,v1))
            or not (0<=u0<u1<=cw and 0<=v0<v1<=ch)):
        raise ValueError('Recognition box is outside the captured model canvas')
    a,b,c,d,e,f=expected
    points=[(a*u+c*v+e,b*u+d*v+f) for u,v in ((u0,v0),(u1,v0),(u1,v1),(u0,v1))]
    return [min(p[0] for p in points),min(p[1] for p in points),max(p[0] for p in points),max(p[1] for p in points)]


def ink_runs(mask,box):
    """All actual nonwhite copy pixels inside a recognizer rectangle."""
    x0,y0,x1,y1=box
    raw=mask.crop(box).tobytes();width=x1-x0
    return [[y+y0,m.start()+x0,m.end()+x0] for y in range(y1-y0)
            for m in re.finditer(b'[^\x00]+',raw[y*width:(y+1)*width])]


def support_bounds(runs):
    return ([min(s[1] for s in runs),min(s[0] for s in runs),max(s[2] for s in runs),max(s[0] for s in runs)+1]
            if runs else None)


def support_overlap(first,second):
    by_row={}
    for y,a,b in first:by_row.setdefault(y,[]).append((a,b))
    return any(max(a,c)<min(b,d) for y,c,d in second for a,b in by_row.get(y,[]))


def collect_words(observations,mask,width,height,turn):
    """Keep all observations; merge only equal complete source pixel footprints."""
    words=[];groups={};unresolved=[];supports=[];result=[]
    for observation in observations:
        item=copy.deepcopy(observation);item['id']=len(result);box=item['source_pixels'];core=item['core']
        if (not pixel_box(box,width,height) or not pixel_box(core,width,height)
                or not isinstance(item['text'],str) or type(item['is_vertical']) is not bool
                or type(item['confidence']) not in (int,float) or not math.isfinite(item['confidence'])
                or not 0<=item['confidence']<=1):
            raise ValueError('Invalid source word observation')
        runs=ink_runs(mask,box);bounds=support_bounds(runs);pixels=sum(b-a for y,a,b in runs)
        item['ink_pixels']=pixels;item['ink_bounds_source_pixels']=bounds;item['word']=None
        if bounds is None:
            item['ownership']='no_copy_ink_unverified'
        else:
            cx,cy=(bounds[0]+bounds[2])/2,(bounds[1]+bounds[3])/2
            item['ownership']='owned_ink' if core[0]<=cx<core[2] and core[1]<=cy<core[3] else 'context_ink'
        if item['ownership']!='owned_ink':
            unresolved.append({'reason':item['ownership'],'observations':[item['id']],'words':[]})
            result.append(item);continue
        digest=sha(json.dumps(runs,separators=(',',':')).encode())
        if digest not in groups:
            groups[digest]=len(words);supports.append(runs)
            words.append({'id':len(words),'text':None,'alternatives':[],
                          'bounds_source_pixels':bounds,'bounds_layout_pixels':layout_box(bounds,width,height,turn),
                          'support_sha256':digest,'support_pixels':pixels,'cell':None,'cells':[],
                          'writing_modes':[],'status':'candidate_unverified'})
        wid=groups[digest];word=words[wid]
        # Hash equality is not used to hide a collision or different support.
        if supports[wid]!=runs:raise ValueError('Different source pixel footprints share a hash')
        alternative=next((a for a in word['alternatives'] if a['text']==item['text']),None)
        if alternative is None:
            alternative={'text':item['text'],'observations':[]};word['alternatives'].append(alternative)
        alternative['observations'].append(item['id'])
        if item['is_vertical'] not in word['writing_modes']:word['writing_modes'].append(item['is_vertical'])
        item['word']=wid;result.append(item)
    for word in words:
        if len(word['alternatives'])==1:word['text']=word['alternatives'][0]['text']
        else:unresolved.append({'reason':'conflicting_text_for_identical_copy_ink','observations':[], 'words':[word['id']]})
    for i,word in enumerate(words):
        for j,other in enumerate(words[:i]):
            if overlap(word['bounds_source_pixels'],other['bounds_source_pixels']) and support_overlap(supports[i],supports[j]):
                unresolved.append({'reason':'overlapping_word_support','observations':[], 'words':[j,i]})
    return words,result,unresolved


def candidate_cells(observation,width,height,turn):
    cells=[]
    ids=set()
    for cell in observation['rectangular_cells']:
        if type(cell['id']) is not int or cell['id']<0 or cell['id'] in ids:
            raise ValueError('Invalid or duplicate source cell identity')
        ids.add(cell['id'])
        hs=cell['horizontal_rules'];vs=cell['vertical_rules']
        if len(hs)!=2 or len(vs)!=2:raise ValueError('Source rectangle needs four ruling tracks')
        top,bottom=[observation['horizontal'][i]['band'] for i in hs]
        left,right=[observation['vertical'][i]['band'] for i in vs]
        frame=[sum(left)/2,sum(top)/2,sum(right)/2,sum(bottom)/2]
        interior=[left[1],top[1],right[0],bottom[0]]
        if cell['interior_pixels']!=interior or not pixel_box(interior,width,height):
            raise ValueError('Source cell interior differs from its ruling bands')
        cells.append({'id':cell['id'],'interior_source_pixels':cell['interior_pixels'],
                      'frame_source_pixels':frame,'frame_layout_pixels':layout_box(frame,width,height,turn),
                      'boundary_kind':cell.get('boundary_kind','solid_source'), 'words':[]})
    return cells


def assign_cells(words,cells):
    unresolved=[]
    for word in words:
        box=word['bounds_source_pixels']
        covering=[c for c in cells if contains(c['interior_source_pixels'],box)]
        inner=[c for c in covering if not any(c['interior_source_pixels']!=other['interior_source_pixels']
               and contains(c['interior_source_pixels'],other['interior_source_pixels']) for other in covering)]
        word['cells']=[c['id'] for c in inner]
        if len(inner)==1:
            word['cell']=inner[0]['id'];inner[0]['words'].append(word['id'])
        elif len(inner)>1:
            unresolved.append({'reason':'ambiguous_cell_containment','observations':[], 'words':[word['id']]})
        elif any(overlap(c['interior_source_pixels'],box) for c in cells):
            unresolved.append({'reason':'word_crosses_candidate_cell_boundary','observations':[], 'words':[word['id']]})
    return unresolved


def cell_tables(cells):
    parents=list(range(len(cells)))
    def root(i):
        while parents[i]!=i:parents[i]=parents[parents[i]];i=parents[i]
        return i
    for i,c in enumerate(cells):
        a=c['frame_layout_pixels']
        for j,other in enumerate(cells[:i]):
            b=other['frame_layout_pixels']
            adjacent=((a[0]==b[2] or a[2]==b[0]) and max(a[1],b[1])<min(a[3],b[3])
                      or (a[1]==b[3] or a[3]==b[1]) and max(a[0],b[0])<min(a[2],b[2]))
            if adjacent or overlap(a,b):parents[root(i)]=root(j)
    groups={}
    for i,c in enumerate(cells):groups.setdefault(root(i),[]).append(c)
    tables=[]
    for group in groups.values():
        xs=sorted({v for c in group for v in (c['frame_layout_pixels'][0],c['frame_layout_pixels'][2])})
        ys=sorted({v for c in group for v in (c['frame_layout_pixels'][1],c['frame_layout_pixels'][3])})
        slots=[];occupied={};conflicts=[]
        for c in group:
            a,b,x,y=c['frame_layout_pixels'];col,row=xs.index(a),ys.index(b);end_col,end_row=xs.index(x),ys.index(y)
            slots.append({'cell':c['id'],'row':row,'col':col,'rowspan':end_row-row,'colspan':end_col-col})
            for yy in range(row,end_row):
                for xx in range(col,end_col):
                    conflicts.extend([other,c['id']] for other in occupied.get((yy,xx),[]))
                    occupied.setdefault((yy,xx),[]).append(c['id'])
        holes=[[y,x] for y in range(len(ys)-1) for x in range(len(xs)-1) if (y,x) not in occupied]
        tables.append({'id':len(tables),'cells':[c['id'] for c in group],'x_cuts':xs,'y_cuts':ys,
                       'slots':slots,'uncovered_slots':holes,'overlapping_cells':conflicts,
                       'status':'geometric_candidates_unverified'})
    return tables


def cell_lines(words,cells,orientation_resolved):
    lines=[]
    for cell in cells:
        members=[words[i] for i in cell['words']];pending=set(w['id'] for w in members)
        while pending:
            ids={min(pending)};changed=True
            while changed:
                changed=False
                for i in sorted(pending-ids):
                    a=words[i]['bounds_layout_pixels']
                    if any(max(a[1],words[j]['bounds_layout_pixels'][1])<min(a[3],words[j]['bounds_layout_pixels'][3]) for j in ids):
                        ids.add(i);changed=True
            pending-=ids;ordered=sorted(ids,key=lambda i:words[i]['bounds_layout_pixels'][0])
            bounds=[words[i]['bounds_layout_pixels'] for i in ids]
            unambiguous=(orientation_resolved and max(b[1] for b in bounds)<min(b[3] for b in bounds)
                         and all(words[i]['writing_modes']==[False] for i in ids)
                         and all(words[i]['text'] is not None for i in ids)
                         and all(words[a]['bounds_layout_pixels'][2]<=words[b]['bounds_layout_pixels'][0]
                                 for a,b in zip(ordered,ordered[1:])))
            lines.append({'id':len(lines),'cell':cell['id'],'words':ordered if unambiguous else sorted(ids),
                          'status':'ordered_disjoint_horizontal_words' if unambiguous else 'reading_order_unresolved'})
    return lines


def read_layout(directory, character_report=None):
    directory=Path(directory);captured=directory/'prepared'
    index_raw=(directory/'index.json').read_bytes();index=json.loads(index_raw)
    prep_raw=(captured/'preparation.json').read_bytes();prep=json.loads(prep_raw)
    if (index.get('schema')!=1 or index.get('kind')!='unclassified_source_derived_image_ocr'
            or index.get('coverage_verified') is not False or index['source']!=prep['source']
            or sha(prep_raw)!=index['preparation_sha256']):
        raise ValueError('OCR index differs from its captured raster preparation')
    for name,digest in [('source.pdf',prep['source']['document_sha256']),('source.png',prep['source']['source_PNG_sha256']),
                        ('recognition-parent.png',prep['recognition_parent_sha256']),('source-placement.json',prep['source']['source_placement_sha256'])]:
        if sha((captured/name).read_bytes())!=digest:raise ValueError('Captured raster layout source changed: '+name)
    parent=Image.open(captured/'recognition-parent.png').convert('RGBA');mask=ink_mask(parent);width,height=parent.size
    original=Image.open(captured/'source.png').convert('RGBA')
    if sha(original.tobytes())!=prep['source']['source_RGBA_sha256']:
        raise ValueError('Original decoded RGBA differs from its source declaration')
    source_frame=json.loads((captured/'source-placement.json').read_bytes())
    if prep['canvas']!={'pixel_width':width,'pixel_height':height}:raise ValueError('Source canvas changed')
    turns=prep.get('settings',{}).get('recognition_quarter_turns',0)
    if type(turns) is not int or not 0<=turns<=3:raise ValueError('Invalid glyph orientation')
    resolved=False
    if prep.get('orientation'):
        orientation_raw=local(captured,prep['orientation']['file']).read_bytes()
        if sha(orientation_raw)!=prep['orientation']['sha256']:raise ValueError('Source orientation changed')
        orientation=json.loads(orientation_raw)
        if orientation['source']!=prep['source'] or orientation['observation']['quarter_turns_ccw']!=turns:
            raise ValueError('Orientation belongs to another source')
        resolved=orientation['observation']['status']=='unanimous_observed_glyph_right_axes'
    elif turns:raise ValueError('Missing source orientation evidence')
    if [r['tile'] for r in index['rows']]!=[t['id'] for t in prep['tiles']]:raise ValueError('Incomplete source tile inventory')
    terminal=json.loads((directory/'terminal.json').read_bytes())
    if terminal.get('status') not in ('complete','partial') or terminal.get('canonical_integration') is not False:
        raise ValueError('Expected terminal unclassified recognition observations')
    observations=[];runs=[];failures=[]
    for row,tile in zip(index['rows'],prep['tiles']):
        if row['status']=='recognition_copy_empty_unverified':
            if tile['status']!='recognition_copy_empty_unverified':raise ValueError('Recognition silently omitted a prepared source tile')
            continue
        if tile['status']!='prepared_unclassified':raise ValueError('Recognition used an undeclared source tile')
        if row['status']!='recognized_unclassified':failures.append({'tile':tile['id'],'reason':row['status']});continue
        projection_raw=local(directory,row['projection']).read_bytes();projection=json.loads(projection_raw)
        derivation=projection['source_derivation']
        if (derivation['preparation_sha256']!=index['preparation_sha256'] or derivation['tile']!=tile['id']
                or derivation['original_document_sha256']!=prep['source']['document_sha256']
                or derivation['original_PNG_sha256']!=prep['source']['source_PNG_sha256']):
            raise ValueError('Recognition projection belongs to another source')
        raw_path=directory/f"tile-{tile['id']:05}-ocr"/'observations.json';raw_bytes=raw_path.read_bytes();raw=json.loads(raw_bytes)
        if (raw.get('kind')!='unclassified_ocr' or raw.get('layout_available') is not True
                or raw.get('article_assignment') is not None or raw.get('native_glyph_binding') is not None):
            raise ValueError('Expected unclassified recognizer regions')
        png=local(captured,tile['PNG']).read_bytes()
        if sha(png)!=tile['PNG_sha256'] or raw['source']['image']['sha256']!=sha(png) or raw['source']['image']['bytes']!=len(png):
            raise ValueError('Actual recognition input differs from the source crop')
        child=Image.open(BytesIO(png)).convert('RGBA')
        if [raw['canvas']['pixel_width'],raw['canvas']['pixel_height']]!=list(child.size):
            raise ValueError('Recognizer canvas differs from its actual input PNG')
        inverse=(None,Image.Transpose.ROTATE_270,Image.Transpose.ROTATE_180,Image.Transpose.ROTATE_90)[turns]
        restored_child=child if turns==0 else child.transpose(inverse)
        expected_crop=parent.crop(tile['recognition_roi'])
        if restored_child.size!=expected_crop.size or restored_child.tobytes()!=expected_crop.tobytes():
            raise ValueError('Recognition pixels do not reproduce the original parent crop')
        frame_raw=local(captured,tile['placement']).read_bytes();frame=json.loads(frame_raw)
        if sha(frame_raw)!=tile['placement_sha256'] or projection['declared_placement']!=frame:
            raise ValueError('Captured recognition placement changed')
        for key in ('image_width','image_height','image_to_user','crop_box','rotate'):
            if frame[key]!=source_frame[key]:raise ValueError('Recognition placement borrowed another image frame')
        a,b,c,d,e,f=frame['recognition_to_image'];aa,bb,cc,dd,ee,ff=source_frame['recognition_to_image']
        x,y,r,bottom=tile['recognition_roi'];w,h=r-x,bottom-y
        for u,v in ((0,0),(child.width,0),(child.width,child.height),(0,child.height)):
            sx,sy=((x+u,y+v),(x+w-v,y+u),(x+w-u,y+h-v),(x+v,y+h-u))[turns]
            if [a*u+c*v+e,b*u+d*v+f]!=[aa*sx+cc*sy+ee,bb*sx+dd*sy+ff]:
                raise ValueError('Recognition placement differs from the source pixel permutation')
        trace_raw=local(raw_path.parent,raw['engine_result']['trace']).read_bytes();trace=json.loads(trace_raw)
        if sha(trace_raw)!=raw['engine_result']['trace_sha256'] or trace['body']!=raw['text']:
            raise ValueError('Raw recognizer trace or body changed')
        expected_regions=[{'id':i,'text':r['text'],'confidence':r['confidence'],'is_vertical':r['is_vertical'],
                           'bbox':{'pixels':dict(zip(('x0','y0','x1','y1'),r['bbox']))}}
                          for i,r in enumerate(trace['stages']['reading_order'])]
        if expected_regions!=raw['recognition_regions'] or '\n'.join(r['text'] for r in expected_regions)!=raw['text']:
            raise ValueError('Raw recognition regions differ from the complete trace')
        if len(raw['recognition_regions'])!=len(projection['recognition_regions']):raise ValueError('Recognition region inventory changed')
        runs.append({'tile':tile['id'],'raw_sha256':sha(raw_bytes),'projection_sha256':sha(projection_raw),
                     'source':raw['source'],'canvas':raw['canvas']})
        for original,region in zip(raw['recognition_regions'],projection['recognition_regions']):
            restored={k:v for k,v in region.items() if k not in ('projection','source_core','source_ink')}
            if restored!=original:raise ValueError('Raw recognition text, confidence or geometry changed')
            bounds=source_box(tile,original['bbox']['pixels'],turns)
            if bounds!=region['source_core']['source_pixels']:raise ValueError('Projected source word coordinates changed')
            observations.append({'tile':tile['id'],'region':original['id'],'text':original['text'],
                                 'confidence':original['confidence'],'is_vertical':original['is_vertical'],
                                 'source_pixels':bounds,'core':tile['core'],'raw':copy.deepcopy(region)})
    words,observations,unresolved=collect_words(observations,mask,width,height,turns)
    declaration=prep.get('partition_observation')
    member=declaration['file'] if declaration else 'source-observation.json'
    geometry_raw=local(captured,member).read_bytes()
    expected=declaration['sha256'] if declaration else prep['source']['observation_sha256']
    if sha(geometry_raw)!=expected:raise ValueError('Source cell observation changed')
    geometry=json.loads(geometry_raw);cells=candidate_cells(geometry,width,height,turns)
    unresolved+=assign_cells(words,cells)
    result = {'schema':1,'kind':'source_derived_raster_layout','source':prep['source'],'canvas':prep['canvas'],
            'source_frame':source_frame,
            'provenance':{'preparation_sha256':sha(prep_raw),'ocr_index_sha256':sha(index_raw),
                          'recognition_parent_sha256':prep['recognition_parent_sha256'],'producer_sha256':sha(Path(__file__).read_bytes())},
            'orientation':{'quarter_turns_ccw':turns,'globally_resolved':resolved},
            'observations':observations,'runs':runs,'words':words,'cells':cells,'tables':cell_tables(cells),
            'lines':cell_lines(words,cells,resolved),'unresolved':unresolved,'failed_tiles':failures,
            'outside_words':[w['id'] for w in words if w['cell'] is None],
            'ownership_policy':'actual_copy_ink_bbox_center_in_half_open_source_core',
            'alias_policy':'exact_global_copy_ink_row_runs_and_exact_raw_text_agreement',
            'coverage_verified':False,'visibility':'unverified'}
    if character_report is not None:
        from raster_characters import character_layer
        result['character_layer'] = character_layer(result, parent, Path(character_report).read_bytes())
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('ocr_directory');parser.add_argument('--output',required=True)
    parser.add_argument('--character-report',help='Source-bound character observer report; retains all raw alternatives')
    args=parser.parse_args();result=read_layout(args.ocr_directory,args.character_report)
    with Path(args.output).open('x') as output:output.write(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({'words':len(result['words']),'cells':len(result['cells']),'tables':len(result['tables']),
                      'unresolved':len(result['unresolved']),'failed_tiles':len(result['failed_tiles'])}))
    return 2 if result['failed_tiles'] else 0


if __name__=='__main__':raise SystemExit(main())
