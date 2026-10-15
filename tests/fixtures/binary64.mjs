import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {createValueCodec} from './value-wire.mjs';
import {TypedJsonCodec} from './json-value.mjs';
import {pack,unpack,width} from './packed.mjs';
const data=JSON.parse(await readFile(new URL('./cases.json',import.meta.url)));
const codec=await createValueCodec({kind:'f64'}),json=new TypedJsonCodec(codec);
assert.deepEqual(codec.descriptor,data.descriptor);assert.equal(width({kind:'f64'}),2);
for(const convert of [v=>codec.capture(v),v=>codec.encode(v),v=>pack(v,{kind:'f64'})]){
 let reads=0;const value={get F64Bits(){assert.equal(++reads,1);return '1';}};convert(value);assert.equal(reads,1);
}
let changingReads=0;assert.throws(()=>json.stringify({get F64Bits(){return ++changingReads===1?'1':'18446744073709551616';}}));
for(const c of data.cases){
 const bytes=Uint8Array.from(c.bytes);
 assert.deepEqual(codec.encode(c.value),bytes);assert.deepEqual(codec.decode(bytes),c.value);
 assert.deepEqual(codec.scan(bytes,{},true,()=>{}),c.value);assert.equal(codec.validate(bytes),true);
 const captured=codec.capture(c.value);assert.deepEqual(captured,c.value);assert.notEqual(captured,c.value);assert.deepEqual(codec.copy(c.value),c.value);
 const compact=codec.compact(),short=compact.encode(c.value);assert.deepEqual(compact.decode(short),c.value);assert.equal(compact.validate(short),true);
 for(const f of [codec,compact]){const b=f.encode(c.value);assert.throws(()=>f.decode(b.subarray(0,b.length-1)));assert.throws(()=>f.decode(b,{nodes:0}));assert.throws(()=>f.encode(c.value,{bytes:0}));}
 const d=pack(c.value,{kind:'f64'});assert.deepEqual([...d.words],c.words);assert.equal(d.finite,c.finite);assert.deepEqual([...pack(unpack(d.words,{kind:'f64'}),{kind:'f64'}).words],c.words);
 for(const count of [1,5]){
  const descriptor={kind:'record',name:'Wrapper'+count,fields:Array.from({length:count},(_,i)=>({name:'field'+i,type:{kind:'f64'}}))},wrapper=await createValueCodec(descriptor),value=Object.fromEntries(descriptor.fields.map(f=>[f.name,c.value]));
  const b=wrapper.encode(value);assert.deepEqual(wrapper.decode(b),value);assert.deepEqual(wrapper.scan(b,{},true,()=>{}),value);assert.equal(wrapper.validate(b),true);
  const tiny=wrapper.compact(),s=tiny.encode(value);assert.deepEqual(tiny.decode(s),value);assert.equal(tiny.validate(s),true);
 }
}
for(const v of [...data.invalid,NaN,Infinity,-Infinity]){
 assert.throws(()=>codec.encode(v));assert.throws(()=>codec.capture(v));assert.throws(()=>codec.validateValue(v));assert.throws(()=>codec.compact().encode(v));assert.throws(()=>pack(v,{kind:'f64'}));
}
for(const text of ['-0','1.0000000000000002','5e-324','1.7976931348623157e308']){
 const v=json.parse(text),bytes=json.parseBinary(text);assert.deepEqual(codec.decode(bytes),v);assert.deepEqual(json.parse(json.stringify(v)),v);assert.deepEqual(json.parse(json.stringifyBinary(bytes)),v);
 assert.deepEqual(codec.compact().decode(json.parseCompact(text)),v);
 if(text==='-0')assert.equal(v.F64Bits,'9223372036854775808');
 if(text==='1.0000000000000002')assert.equal(v.F64Bits,'4607182418800017409');
}
for(const text of ['1e309','NaN','Infinity','null','{"F64Bits":"0"}']){assert.throws(()=>json.parse(text));assert.throws(()=>json.parseBinary(text));}
for(const value of [{F64Bits:'9218868437227405312'},{F64Bits:'9218868437227409972'}]){assert.throws(()=>json.stringify(value));assert.throws(()=>json.stringifyBinary(codec.encode(value)));}
