"""Lossless pixel partition from native strokes and source-image gutters.

The explicit writing mode is a caller assumption, not language/Article detection.
Neither components nor vector strokes certify final PDF visibility or OCR text.
"""
import math
import re
from PIL import ImageChops,ImageOps

def component_bounds(mask):
    """Bounding boxes of exact nonzero 8-connected pixels, using row runs."""
    if mask.mode!='L':
        raise ValueError('Expected an L-mode nonwhite mask')
    width,height=mask.size;pixels=mask.tobytes();parents=[];ranks=[];bounds=[];previous=[]
    def root(i):
        while parents[i]!=i:
            parents[i]=parents[parents[i]];i=parents[i]
        return i
    def union(a,b):
        a,b=root(a),root(b)
        if a==b:return
        if ranks[a]<ranks[b]:a,b=b,a
        parents[b]=a
        if ranks[a]==ranks[b]:ranks[a]+=1
        aa,bb=bounds[a],bounds[b]
        bounds[a]=[min(aa[0],bb[0]),min(aa[1],bb[1]),max(aa[2],bb[2]),max(aa[3],bb[3])]
    for y in range(height):
        current=[];start=0
        for run in re.finditer(b'[^\x00]+',pixels[y*width:(y+1)*width]):
            lo,hi=run.span();i=len(parents);parents.append(i);ranks.append(0);bounds.append([lo,y,hi,y+1])
            while start<len(previous)and previous[start][1]<lo:start+=1
            j=start
            while j<len(previous)and previous[j][0]<=hi:
                union(i,previous[j][2]);j+=1
            current.append((lo,hi,i))
        previous=current
    return [bounds[i]for i in range(len(parents))if root(i)==i]

def ink_mask(image):
    r,g,b,a=image.convert('RGBA').split()
    nonwhite=ImageChops.lighter(ImageChops.lighter(ImageOps.invert(r),ImageOps.invert(g)),ImageOps.invert(b))
    return ImageChops.darker(nonwhite.point(lambda v:255 if v else 0),a.point(lambda v:255 if v else 0))

def finite(value):
    return type(value)in(int,float)and math.isfinite(value)

def plan(image,paths,page_quad,writing_mode):
    if writing_mode!='horizontal_ltr':
        raise ValueError('Explicit horizontal_ltr writing mode required')
    width,height=image.size
    if width<=0 or height<=0:raise ValueError('Expected a nonempty source image')
    result={'status':'full_image','reason':None,'cuts':[],'rectangles':[[0,0,width,height]],'writing_mode':writing_mode,'visibility':'unverified','article_assignment':None}
    if not(isinstance(page_quad,(list,tuple))and len(page_quad)==4 and all(isinstance(p,(list,tuple))and len(p)==2 and all(finite(v)for v in p)for p in page_quad)):
        result['reason']='invalid_page_quad';return result
    (x0,y0),(x1,top),(right,y1),(left,bottom)=page_quad
    if not(x0<x1 and y0<y1 and top==y0 and right==x1 and left==x0 and bottom==y1):
        result['reason']='source_axes_not_display_aligned';return result
    dx,dy=x1-x0,y1-y0
    if not(math.isfinite(dx)and math.isfinite(dy)):
        result['reason']='unrepresentable_page_quad';return result
    mask=ink_mask(image);ink_bounds=mask.getbbox();components=component_bounds(mask)
    result['nonwhite_component_count']=len(components);result['nonwhite_pixel_bounds']=ink_bounds
    if not components:
        result['reason']='no_nonwhite_source_pixels';return result
    projection=mask.getprojection()[0];gutters=[];begin=None
    for x,painted in enumerate([*projection,1]):
        if not painted and begin is None:begin=x
        if painted and begin is not None:
            if begin>ink_bounds[0]and x<ink_bounds[2]:gutters.append((begin,x))
            begin=None
    low=y0+min(box[3]for box in components)/height*dy
    high=y0+max(box[1]for box in components)/height*dy
    result['component_interval_constraints_page']={'stroke_begin_at_most':low,'stroke_end_at_least':high}
    by_gutter={}
    for pi,path in enumerate(paths):
        line_width=path.get('line_width')
        if path.get('stroke')is not True or path.get('fill')is not False or path.get('clip')is not None or not finite(line_width)or line_width<0:continue
        for si,sub in enumerate(path.get('subpaths',[])):
            points=sub.get('points')
            if sub.get('curved')is not False or not isinstance(points,(list,tuple))or len(points)<2:continue
            if not all(isinstance(p,(list,tuple))and len(p)==2 and all(finite(v)for v in p)for p in points):continue
            segments=list(zip(points,points[1:]))
            if sub.get('closed')is True:segments.append((points[-1],points[0]))
            for ei,((ax,ay),(bx,by))in enumerate(segments):
                if ax!=bx or ay==by or min(ay,by)>low or max(ay,by)<high:continue
                center=(ax-x0)/dx*width;half=line_width/dx*width/2
                if not math.isfinite(center)or not math.isfinite(half):continue
                band=[center-half,center+half]
                for gutter in gutters:
                    if gutter[0]<=band[0]and band[1]<=gutter[1]:
                        by_gutter.setdefault(gutter,[]).append({'path':pi,'subpath':si,'segment':ei,'page_segment':[[ax,ay],[bx,by]],'source_pixel_stroke_band':band})
    for gutter,witnesses in sorted(by_gutter.items()):
        cut=(gutter[0]+gutter[1])//2
        if not 0<cut<width:continue
        result['cuts'].append({'pixel_edge':cut,'white_gutter_pixels':list(gutter),'witnesses':witnesses})
    if not result['cuts']:
        result['reason']='no_source_stroke_in_full_height_white_gutter';return result
    edges=[0,*[cut['pixel_edge']for cut in result['cuts']],width]
    result['rectangles']=[[a,0,b,height]for a,b in zip(edges,edges[1:])]
    result['status']='split'
    return result
