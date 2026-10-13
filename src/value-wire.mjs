// Shared bounded binary codec; generated code supplies exact descriptors.
export const valueWireFormat='ink-value-wire-v1';
const wireMagic=Uint8Array.of(73,78,75,86,65,76,0,1);
const wireEncoder=new TextEncoder(),wireDecoder=new TextDecoder('utf-8',{fatal:true,ignoreBOM:true});
const wireDefaults={bytes:16777216,nodes:1000000,depth:128};
const wireDecimal=s=>typeof s==='string'&&/^(0|[1-9][0-9]*|-[1-9][0-9]*)$/.test(s);
function wireLimits(options){const l={...wireDefaults,...options};for(const k of ['bytes','nodes','depth'])if(!Number.isSafeInteger(l[k])||l[k]<0||l[k]>wireDefaults[k])throw Error('wire hard resource limit');return l;}
const wireFloat=new DataView(new ArrayBuffer(4));
function wireUtf8Length(text){let bytes=0;for(let i=0;i<text.length;i++){const c=text.charCodeAt(i);if(c<128)bytes++;else if(c<2048)bytes+=2;else if(c>=0xd800&&c<=0xdbff){bytes+=4;i++;}else bytes+=3;}return bytes;}
const wireOwn=(v,k)=>v!==null&&typeof v==='object'&&Object.hasOwn(v,k);
const wireTagged=(v,k)=>wireOwn(v,k)&&Object.keys(v).length===1;
const wireCanonical=v=>Array.isArray(v)?'['+v.map(wireCanonical).join(',')+']':v&&typeof v==='object'?'{'+Object.keys(v).sort((a,b)=>{const x=Array.from(a,c=>c.codePointAt(0)),y=Array.from(b,c=>c.codePointAt(0));for(let i=0;i<Math.min(x.length,y.length);i++)if(x[i]!==y[i])return x[i]-y[i];return x.length-y.length;}).map(k=>JSON.stringify(k)+':'+wireCanonical(v[k])).join(',')+'}':JSON.stringify(v);
export async function createValueCodec(root,named={}){
 const used=new Set(),budget={nodes:100000};
 function check(t,d=0){
  if(d>64||--budget.nodes<0)throw Error('wire descriptor limit');
  const keys={u32:['kind'],u64:['kind'],i32:['kind'],f32:['kind'],bool:['kind'],unit:['kind'],string:['kind'],int:['kind'],id:['kind','name'],ref:['kind','name'],enum:['kind','name','variants'],record:['kind','name','fields'],list:['kind','element'],option:['kind','element'],vector:['kind','element','size'],result:['kind','ok','error']}[t?.kind];
  if(!keys||Object.keys(t).length!==keys.length||keys.some(k=>!Object.hasOwn(t,k)))throw Error('invalid wire descriptor');
  if(keys.includes('name')&&(typeof t.name!=='string'||!t.name.isWellFormed()))throw Error('invalid descriptor name');
  if(t.kind==='ref'){if(!used.has(t.name)){used.add(t.name);if(named[t.name]?.kind!=='record'||named[t.name].name!==t.name)throw Error('unresolved wire reference');check(named[t.name],d+1);}}
  if(t.kind==='record'){if(!Array.isArray(t.fields))throw Error('wire fields');const names=new Set();for(const f of t.fields){if(Object.keys(f).length!==2||typeof f.name!=='string'||!f.name.isWellFormed()||names.has(f.name))throw Error('duplicate wire field');names.add(f.name);check(f.type,d+1);}}
  if(t.kind==='enum'&&(!Array.isArray(t.variants)||!t.variants.length||t.variants.length>64||t.variants.some(v=>typeof v!=='string'||!v.isWellFormed())||new Set(t.variants).size!==t.variants.length))throw Error('wire enum');
  if(['list','vector','option'].includes(t.kind)){if(t.kind==='vector'&&(!Number.isInteger(t.size)||t.size<1||t.size>64))throw Error('wire vector');check(t.element,d+1);}
  if(t.kind==='result'){check(t.ok,d+1);check(t.error,d+1);}
 }
 check(root);const names=Object.fromEntries([...used].sort().map(k=>[k,named[k]]));
 const hash=new Uint8Array(await crypto.subtle.digest('SHA-256',wireEncoder.encode(wireCanonical({format:valueWireFormat,root,named:names}))));
 return new ValueCodec({format:valueWireFormat,root,named:names,identity:Array.from(hash,b=>b.toString(16).padStart(2,'0')).join('')});
}
export class ValueCodec{
 #hash;
 #descriptor;
 get descriptor(){return this.#descriptor;}
 constructor(descriptor){
  // This synchronous path is for compiler-generated immutable metadata. For
  // untrusted descriptors use createValueCodec and compare the computed identity.
  if(descriptor?.format!==valueWireFormat||!/^[0-9a-f]{64}$/.test(descriptor.identity))throw Error('invalid wire schema');
  this.#descriptor=structuredClone(descriptor);this.#hash=Uint8Array.from(descriptor.identity.match(/../g),s=>parseInt(s,16));
  const freeze=v=>{if(v&&typeof v==='object'){Object.values(v).forEach(freeze);Object.freeze(v);}};freeze(this.descriptor);
 }
 resolve(t){for(let i=0;i<64;i++){if(t.kind!=='ref')return t;t=this.descriptor.named[t.name];if(!t)throw Error('wire reference');}throw Error('wire reference cycle');}
 validateValue(value,options={}){this.#walkValue(value,options,null);return true;}
 encode(value,options={}){
  const w=new WireEncoder(options);w.bytes(wireMagic);w.bytes(this.#hash);this.#walkValue(value,options,w);return w.finish();
 }
 #walkValue(value,options,w){
  const l=wireLimits(options);let remaining=l.nodes,total=40;
  const add=n=>{total+=n;if(total>l.bytes)throw Error('wire byte limit');};
  const node=(type,v,d)=>{
   if(d>l.depth||--remaining<0)throw Error('wire node or depth limit');const t=this.resolve(type),start=w?.position;
   if(w)w.word(0);
   const bad=()=>{throw Error('invalid value for wire schema');};
   switch(t.kind){
    case 'unit':if(v!==null)bad();add(4);break;
    case 'bool':if(typeof v!=='boolean')bad();add(5);if(w)w.byte(+v);break;
    case 'u32':case 'i32':{
     const min=t.kind==='u32'?0:-2147483648,max=t.kind==='u32'?4294967295:2147483647;
     if(typeof v!=='number'||!Number.isInteger(v)||v<min||v>max)bad();add(8);if(w)w.word(v>>>0);break;
    }
    case 'u64':{
     if(typeof v==='number'&&!Number.isSafeInteger(v)||!['bigint','number','string'].includes(typeof v)||typeof v==='string'&&(!wireDecimal(v)||v.startsWith('-')))bad();
     const n=BigInt(v);if(n<0n||n>18446744073709551615n)bad();add(12);if(w)w.wide(n);break;
    }
    case 'f32':{
     let bits;
     if(typeof v==='number'&&Number.isFinite(v)){wireFloat.setFloat32(0,v,true);if(!Number.isFinite(wireFloat.getFloat32(0,true)))bad();bits=wireFloat.getUint32(0,true);}
     else{if(!wireTagged(v,'F32Bits'))bad();bits=v.F32Bits;if(!Number.isInteger(bits)||bits<0||bits>4294967295)bad();}
     add(8);if(w)w.word(bits);break;
    }
    case 'string':case 'int':case 'id':case 'enum':{
     const text=t.kind==='int'?(typeof v==='bigint'?v.toString():wireTagged(v,'Int')?v.Int:null):v;
     if(typeof text!=='string'||!text.isWellFormed()||text.length>l.bytes)bad();
     if(t.kind==='int'&&!wireDecimal(text)||t.kind==='id'&&!/^[0-9a-f]{32}$/.test(text)||t.kind==='enum'&&!t.variants.includes(text))bad();
     add(4+wireUtf8Length(text));if(w)w.text(text);break;
    }
    case 'list':case 'vector':{
     if(!Array.isArray(v)&&!(ArrayBuffer.isView(v)&&!(v instanceof DataView)))bad();if(v.length>remaining||t.kind==='vector'&&v.length!==t.size)bad();
     add(8);if(w)w.word(v.length);for(let i=0;i<v.length;i++)node(t.element,v[i],d+1);break;
    }
    case 'record':{
     if(!v||typeof v!=='object'||Array.isArray(v)||Object.keys(v).length!==t.fields.length||t.fields.some(f=>!wireOwn(v,f.name)))bad();
     add(4);for(const f of t.fields)node(f.type,v[f.name],d+1);break;
    }
    case 'option':case 'result':{
     let tag,child;
     if(t.kind==='option'){
      if(wireTagged(v,'None')&&v.None===null)tag=0;
      else if(wireTagged(v,'Some')){tag=1;child=t.element;}else bad();
     }else if(wireTagged(v,'Ok')){tag=0;child=t.ok;}else if(wireTagged(v,'Err')){tag=1;child=t.error;}else bad();
     add(5);if(w)w.byte(tag);if(child)node(child,v[t.kind==='option'?'Some':tag?'Err':'Ok'],d+1);break;
    }
    default:throw Error('unsupported wire kind');
   }
   if(w)w.patchWord(start,w.position-start-4);
  };
  node(this.descriptor.root,value,0);if(total>l.bytes)throw Error('wire byte limit');
 }
 encodeWith(emit,options={}){
  const w=new WireEncoder(options);w.bytes(wireMagic);w.bytes(this.#hash);emit(w);const bytes=w.finish();this.validate(bytes,options);return bytes;
 }
 scan(bytes,options={},materialize=false,visitor=null){
  const l=wireLimits(options);if(!(bytes instanceof Uint8Array)||typeof SharedArrayBuffer==='function'&&bytes.buffer instanceof SharedArrayBuffer||bytes.length<44||bytes.length>l.bytes)throw Error('wire byte limit or header');
  if(wireMagic.some((b,i)=>bytes[i]!==b)||this.#hash.some((b,i)=>bytes[i+8]!==b))throw Error('wire format or schema mismatch');
  let remaining=l.nodes;const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
  const node=(t,at,parentEnd,d)=>{
   if(d>l.depth||--remaining<0||at+4>parentEnd)throw Error('wire node, depth or length limit');t=this.resolve(t);const start=at+4,end=start+view.getUint32(at,true);if(end>parentEnd)throw Error('truncated wire frame');let p=start,value;
   const size=n=>{if(end-start!==n)throw Error('malformed wire scalar');};
   const child=type=>{const r=node(type,p,end,d+1);p=r.end;return r.value;};
   switch(t.kind){
    case 'unit':size(0);value=null;break;
    case 'bool':size(1);if(bytes[start]>1)throw Error('invalid wire Bool');value=!!bytes[start];break;
    case 'u32':size(4);value=view.getUint32(start,true);break;
    case 'i32':size(4);value=view.getInt32(start,true);break;
    case 'f32':size(4);if(materialize)value={F32Bits:view.getUint32(start,true)};break;
    case 'u64':size(8);if(materialize)value=view.getBigUint64(start,true);break;
    case 'string':case 'int':case 'id':case 'enum':{if(t.kind==='string'&&!materialize){let i=start;while(i<end&&bytes[i]<128)i++;if(i<end)wireDecoder.decode(bytes.subarray(start,end));break;}const text=wireDecoder.decode(bytes.subarray(start,end));if(t.kind==='int'&&!wireDecimal(text)||t.kind==='id'&&!/^[0-9a-f]{32}$/.test(text)||t.kind==='enum'&&!t.variants.includes(text))throw Error('malformed wire text');if(materialize)value=t.kind==='int'?{Int:text}:text;break;}
    case 'list':case 'vector':{if(start+4>end)throw Error('wire list count');const n=view.getUint32(start,true);p+=4;if(n>remaining||n>Math.floor((end-p)/4)||t.kind==='vector'&&n!==t.size)throw Error('wire sequence size');if(materialize)value=[];for(let i=0;i<n;i++){const v=child(t.element);if(materialize)value.push(v);}if(p!==end)throw Error('trailing sequence bytes');break;}
    case 'record':{if(materialize)value={};for(const f of t.fields){const v=child(f.type);if(materialize)Object.defineProperty(value,f.name,{value:v,enumerable:true,writable:true,configurable:true});}if(p!==end)throw Error('trailing record bytes');break;}
    case 'option':case 'result':{if(start===end||bytes[p]>1)throw Error('wire sum tag');const b=bytes[p++];if(t.kind==='option'&&b===0){if(p!==end)throw Error('wire None bytes');if(materialize)value={None:null};}else{const tag=t.kind==='option'?'Some':b===0?'Ok':'Err';const v=child(t.kind==='option'?t.element:b===0?t.ok:t.error);if(p!==end)throw Error('trailing sum bytes');if(materialize)value={[tag]:v};}break;}
    default:throw Error('unsupported wire kind');
   }if(visitor)visitor(t,bytes.subarray(start,end),d);return {end,value};
  };
  const r=node(this.descriptor.root,40,bytes.length,0);if(r.end!==bytes.length)throw Error('trailing wire bytes');return r.value;
 }
 visit(bytes,visitor,options={}){if(typeof visitor!=='function')throw Error('wire visitor required');this.scan(bytes,options,false,visitor);}
 validate(bytes,options={}){this.scan(bytes,options,false);return true;}
 decode(bytes,options={}){return this.scan(bytes,options,true);}
}
function join(parts){const size=parts.reduce((n,p)=>n+p.length,0);if(size>16777216)throw Error('wire allocation limit');const out=new Uint8Array(size);let at=0;for(const p of parts){out.set(p,at);at+=p.length;}return out;}

// Bounded forward emission avoids building and repeatedly joining child buffers.
class WireEncoder{
 #data=new Uint8Array(256);#view=new DataView(this.#data.buffer);#at=0;#remaining;#depth=0;#limits;
 constructor(options){this.#limits=wireLimits(options);this.#remaining=this.#limits.nodes;}
 get position(){return this.#at;}
 #reserve(size){
  const end=this.#at+size;if(end>this.#limits.bytes)throw Error('wire byte limit');
  if(end>this.#data.length){const next=new Uint8Array(Math.min(this.#limits.bytes,Math.max(end,this.#data.length*2)));next.set(this.#data.subarray(0,this.#at));this.#data=next;this.#view=new DataView(next.buffer);}
  return end;
 }
 bytes(bytes){const end=this.#reserve(bytes.length);this.#data.set(bytes,this.#at);this.#at=end;}
 byte(n){if(!Number.isInteger(n)||n<0||n>255)throw Error('wire byte range');this.#reserve(1);this.#data[this.#at++]=n;}
 word(n){if(!Number.isInteger(n)||n<0||n>4294967295)throw Error('wire word range');const end=this.#reserve(4);this.#view.setUint32(this.#at,n,true);this.#at=end;}
 wide(n){if(typeof n!=='bigint'||n<0n||n>18446744073709551615n)throw Error('wire wide range');const end=this.#reserve(8);this.#view.setBigUint64(this.#at,n,true);this.#at=end;}
 text(text){if(typeof text!=='string'||!text.isWellFormed())throw Error('wire Unicode');const size=wireUtf8Length(text),end=this.#reserve(size),r=wireEncoder.encodeInto(text,this.#data.subarray(this.#at,end));if(r.read!==text.length||r.written!==size)throw Error('wire UTF-8 size');this.#at=end;}
 patchWord(at,n){if(!Number.isInteger(at)||at<0||at+4>this.#at||!Number.isInteger(n)||n<0||n>4294967295)throw Error('wire patch offset');this.#view.setUint32(at,n,true);}
 frame(emit){if(this.#depth>this.#limits.depth||--this.#remaining<0)throw Error('wire node or depth limit');const at=this.position;this.word(0);this.#depth++;try{emit(this);}finally{this.#depth--;}this.patchWord(at,this.position-at-4);}
 reorder(start,spans){
  const sorted=spans.slice().sort((a,b)=>a[0]-b[0]);let at=start;
  for(const [a,b] of sorted){if(!Number.isInteger(a)||!Number.isInteger(b)||a!==at||b<a)throw Error('wire span permutation');at=b;}
  if(start<0||start>this.#at||at!==this.#at)throw Error('wire span permutation');
  if(spans.every((s,i)=>s===sorted[i]))return;
  const copy=this.#data.slice(start,this.#at);at=start;for(const [a,b] of spans){this.#data.set(copy.subarray(a-start,b-start),at);at+=b-a;}
 }
 finish(){return this.#data.slice(0,this.#at);}
}
