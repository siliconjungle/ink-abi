import assert from 'node:assert/strict';
import {pack,unpack,numericDestination} from './packed.mjs';
const word={kind:'list',element:{kind:'u32'}},wide={kind:'list',element:{kind:'u64'}},float={kind:'list',element:{kind:'f32'}};
const proxy=new Proxy(Uint32Array.of(0xffffffff,3),{ownKeys(){throw Error('do not enumerate packed words')},get(target,key){const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;}});
assert.deepEqual(Array.from(pack(proxy,word).words),[0xffffffff,3]);
const output=new BigUint64Array([0n,0n,77n]);
unpack(Uint32Array.of(0,0x200000,0xffffffff,0xffffffff),wide,true,output);
assert.deepEqual(output,new BigUint64Array([9007199254740992n,18446744073709551615n,77n]));
const short=new BigUint64Array([42n]);assert.throws(()=>unpack(Uint32Array.of(0,1,0,2),wide,true,short),/capacity/);assert.equal(short[0],42n);
const bits=[0,0,'tail'];unpack(Uint32Array.of(0x80000000,0x7fc12345),float,true,bits);
assert.ok(Object.is(bits[0],-0));assert.deepEqual(bits[1],{F32Bits:0x7fc12345});assert.equal(bits[2],'tail');
// Numeric destinations retain all raw float bits, including signaling NaNs,
// infinities and signed zero. Nonzero view offsets and tails remain intact.
const words=Uint32Array.of(0x80000000,1,0x7f800000,0xff800000,0x7fa12345,0xffc54321);
const backing=new Uint32Array(words.length+4).fill(0x12345678),floats=new Float32Array(backing.buffer,8,words.length+1);
unpack(words,float,false,floats);assert.deepEqual(Array.from(backing),[0x12345678,0x12345678,...words,0x12345678,0x12345678]);
const saved=Array.from(backing);assert.throws(()=>unpack(words,float,false,new Float32Array(1)),/capacity/);assert.deepEqual(Array.from(backing),saved);
const narrow=new Int32Array([-1,7,88]);unpack(Uint32Array.of(0x80000000,0xffffffff),{kind:'list',element:{kind:'i32'}},false,narrow);assert.deepEqual(narrow,new Int32Array([-2147483648,-1,88]));
const doubles=new Float64Array(3),doubleWords=Uint32Array.of(0x12345678,0x7ff81234,0,0x80000000);
unpack(doubleWords,{kind:'list',element:{kind:'f64'}},false,doubles);assert.deepEqual(Array.from(new Uint32Array(doubles.buffer)),[...doubleWords,0,0]);
assert.throws(()=>numericDestination(float,new Uint32Array(8)),/dtype/);
assert.throws(()=>numericDestination({kind:'list',element:{kind:'bool'}},new Uint32Array(8)),/dtype/);
assert.throws(()=>numericDestination({kind:'f32'},new Float32Array(8)),/dtype/);
// Own property shadows cannot redirect the raw copy or falsify capacity.
const shadow=new Float32Array(2);Object.defineProperties(shadow,{length:{value:999},buffer:{get(){throw Error('shadow buffer accessed')}}});
numericDestination(float,shadow,2).set(words.subarray(0,2));assert.throws(()=>numericDestination(float,shadow,3),/capacity/);
const moved=new Float32Array(1);structuredClone(moved.buffer,{transfer:[moved.buffer]});assert.throws(()=>numericDestination(float,moved,1));
