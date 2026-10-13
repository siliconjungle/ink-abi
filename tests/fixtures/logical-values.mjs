import assert from 'node:assert/strict';
import {createValueCodec} from './value-wire.mjs';
const scalar=k=>({kind:k});
const node={kind:'record',name:'Node',fields:[{name:'text',type:scalar('string')},{name:'next',type:{kind:'option',element:{kind:'ref',name:'Node'}}}]};
const types=[...['unit','bool','u32','i32','u64','int','f32','string'].map(scalar),
 {kind:'id',name:'Key'},{kind:'enum',name:'Mode',variants:['On','Off']},
 {kind:'vector',size:2,element:scalar('u32')},
 {kind:'list',element:scalar('u32')},
 {kind:'result',ok:scalar('u32'),error:scalar('string')},
 {kind:'record',name:'Empty',fields:[]},
 {kind:'record',name:'Proto',fields:[{name:'__proto__',type:scalar('string')}]},node];
const samples=[null,true,false,0,-0,1,-1,4294967295,4294967296,-2147483648,-2147483649,
 18446744073709551615n,18446744073709551616n,9007199254740992,3.5,1e100,
 '0','01','-1','On','Off','0123456789abcdef0123456789abcdef','\ufeff雪😀','\ud800',
 {Int:'-12345678901234567890'},{Int:'-0'},{F32Bits:0x7fc12345},{F32Bits:0x100000000},
 [],[0,1],new Uint32Array([0,1]),new DataView(new ArrayBuffer(8)),[0,-1],
 {},{Ok:1},{Err:'雪'},{Ok:1,Err:'bad'},{None:null},{Some:1},{['__proto__']:'own'},
 {text:'a',next:{Some:{text:'\ufeff雪😀',next:{None:null}}}},
 {text:'a',next:{Some:{text:'b',next:{Some:{text:'c',next:{None:null}}}}}},
 {text:'a',next:{Some:{text:'b',next:{None:null}}},extra:1}];
// Existing tagged carrier semantics ignore inherited and non-enumerable extras.
const inherited=Object.assign(Object.create({extra:1}),{Ok:1});samples.push(inherited);
const hidden={Ok:1};Object.defineProperty(hidden,'extra',{value:1});samples.push(hidden);
const outcome=(run,value,limits)=>{try{run(value,limits);return true;}catch{return false;}};
for(const type of types){
 const codec=await createValueCodec(type,type===node?{Node:node}:{});
 for(const value of samples){
  assert.equal(outcome((v,l)=>codec.validateValue(v,l),value,{}),outcome((v,l)=>codec.encode(v,l),value,{}));
  if(!outcome(v=>codec.encode(v),value,{}))continue;
  const bytes=codec.encode(value);
  assert.equal(codec.validate(bytes),true);assert.deepEqual(codec.encode(codec.decode(bytes)),bytes);
  for(const limits of [{bytes:bytes.length},{bytes:bytes.length-1},{nodes:0},{nodes:1},{nodes:2},{nodes:4},{depth:0},{depth:1},{depth:3},{depth:6}])
   assert.equal(outcome((v,l)=>codec.validateValue(v,l),value,limits),outcome((v,l)=>codec.encode(v,l),value,limits),JSON.stringify(limits));
 }
}
// A recursive cycle must reject through the node/depth limits, never loop forever.
const codec=await createValueCodec(node,{Node:node}),cycle={text:'loop'};cycle.next={Some:cycle};
assert.throws(()=>codec.validateValue(cycle));
assert.throws(()=>codec.validateValue({text:'a',next:{None:null}},{depth:129}));
console.log('logical value validation agrees with independent forward encoding');

// Capture a raw scalar field once, including ordinary getter-backed inputs.
const float=await createValueCodec(scalar('f32'));let reads=0;const getter={get F32Bits(){return reads++===0?0:4294967296;}};assert.equal(float.validateValue(getter),true);assert.equal(reads,1);

// JSON construction/rendering may fuse schema checking only when the complete
// recursive schema agrees. Compare acceptance with the separate general checker.
const {JsonCodec,TypedJsonCodec,parseJson,stringifyJson}=await import('./json-value.mjs');
const ref={kind:'ref',name:'Json'},record=(name,type)=>({kind:'record',name,fields:type?[{name:'item0',type}]:[]});
const member={kind:'record',name:'Member',fields:[{name:'key',type:scalar('string')},{name:'value',type:ref}]};
const branches=[record('Null'),record('Bool',scalar('bool')),record('Number',scalar('string')),record('String',scalar('string')),record('Array',{kind:'list',element:ref}),record('Object',{kind:'list',element:member})];
let payload=branches[5];for(let i=4;i>=0;i--)payload={kind:'result',ok:branches[i],error:payload};
const jsonType={kind:'record',name:'Json',fields:[{name:'payload',type:payload}]};
const values=await createValueCodec(jsonType,{Json:jsonType}),json=new JsonCodec(values);
const texts=['null','true','false','0','-0','1e10000','"\\ufeff雪😀"','[]','{}','[null,false,0,"x"]','{"a":1,"unknown":{"nested":[true,1e10000]}}','[{"x":[false,null]}]'];
for(const text of texts){
 const value=parseJson(text),bytes=values.encode(value).length,limits=[{},...[0,1,2,3,4,6,7,8,9,15,32].map(nodes=>({nodes})),...[0,1,2,4,7,8,10,16,128].map(depth=>({depth})),...[0,40,44,bytes-1,bytes,bytes+1].map(bytes=>({bytes}))];
 assert.deepEqual(json.parse(text),value);assert.equal(json.stringify(value),stringifyJson(value));
 for(const l of limits){
  const expectedParse=outcome(t=>{const v=parseJson(t,l);values.validateValue(v,l);},text,l);
  assert.equal(outcome(t=>json.parse(t,l),text,l),expectedParse,`${text}: parse ${JSON.stringify(l)}`);
  const expectedRender=outcome(v=>{values.validateValue(v,l);stringifyJson(v,l);},value,l);
  assert.equal(outcome(v=>json.stringify(v,l),value,l),expectedRender,`${text}: render ${JSON.stringify(l)}`);
 }
}
for(const text of ['{"a":1,"\\u0061":2}','"\\ud800"','[0,]','01','null trailing'])assert.throws(()=>json.parse(text));
const mismatched=await createValueCodec(jsonType,{Json:{kind:'record',name:'Json',fields:[{name:'wrong',type:scalar('string')}]}});
assert.throws(()=>new JsonCodec(mismatched));
const inheritedMember={value:parseJson('0'),extra:1};Object.setPrototypeOf(inheritedMember,{key:'inherited'});
const object=parseJson('{"x":0}');object.payload.Err.Err.Err.Err.Err.item0=[inheritedMember];assert.throws(()=>json.stringify(object));
const typed=new TypedJsonCodec(await createValueCodec({kind:'record',name:'Words',fields:[{name:'wide',type:scalar('u64')},{name:'words',type:{kind:'list',element:scalar('int')}}]}));
const words=typed.parse('{"wide":18446744073709551615,"words":[1e2,-2]}');assert.equal(words.wide,18446744073709551615n);assert.deepEqual(words.words,[{Int:'100'},{Int:'-2'}]);assert.equal(typed.parse(typed.stringify(words)).wide,words.wide);
console.log('fused JSON boundaries preserve recursive schemas and exact wire budgets');

const ambiguous=parseJson('true');Object.defineProperty(ambiguous.payload,'Ok',{value:{}});assert.throws(()=>json.stringify(ambiguous));

const observed=[],projected=[],sample=values.encode(parseJson('{\"a\":1,\"b\":[2]}'));
values.visit(sample,(t,b,d)=>observed.push([t.name,Array.from(b),d]));
values.visit(sample,(t,b,d)=>projected.push([t.name,Array.from(b),d]),{},['Number','Object']);
assert.deepEqual(projected,observed.filter(([name])=>name==='Number'||name==='Object'));
let callbacks=0;values.visit(sample,()=>callbacks++,{},[]);assert.equal(callbacks,0);
const badUnicode=values.encode(parseJson('\"x\"'));badUnicode[badUnicode.length-1]=255;assert.throws(()=>values.visit(badUnicode,()=>{}, {},[]));

// Small-record decoding must preserve own data properties and ignore setters.
const ordinary={key:"x",value:parseJson('1')};Object.defineProperty(Object.prototype,'item0',{set(){throw Error('prototype setter called');},configurable:true});
try{assert.deepEqual(values.decode(values.encode(parseJson('[1,true,\"x\"]'))),parseJson('[1,true,\"x\"]'));}finally{delete Object.prototype.item0;}

const padded=new Uint8Array(sample.length+7);padded.set(sample,7);const sliced=padded.subarray(7),spans=[];values.visitSpans(sliced,(t,data,lo,hi,d)=>spans.push([t.name,Array.from(data.subarray(lo,hi)),d]),{},['Number','Object']);assert.deepEqual(spans,projected);assert.equal(json.validateBinary(sliced),true);
for(const token of ['0','-0','1.2e+300','1e10000','01','-','1.','1e','1e+','+1','1\n','\ufeff1','雪','']){const v=parseJson('1');v.payload.Err.Err.Ok.item0=token;const bytes=values.encode(v);assert.equal(outcome(()=>json.validateBinary(bytes)),outcome(()=>json.stringify(v)),token);}
