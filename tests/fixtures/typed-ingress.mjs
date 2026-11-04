import assert from 'node:assert/strict';
import {capture,captureTransport,pack} from './packed.mjs';
const type=Object.freeze({kind:'list',element:Object.freeze({kind:'f32'})});
const patterns=[0,0x80000000,1,0x007fffff,0x00800000,0x3eaaaaab,0x7f7fffff,0xff7fffff];
for(const count of [0,1,7,128]){
 const bits=Uint32Array.from({length:count+4},(_,i)=>patterns[i%patterns.length]);
 const value=new Float32Array(bits.buffer,8,count),expected=bits.slice(2,2+count);
 const packed=pack(value,type),owned=captureTransport(value,type),native=capture(value,type);
 assert.equal(Object.hasOwn(native,'words'),false);assert(Array.isArray(native.javascript));
 assert.deepEqual(packed.words,expected);
 if(count>=128)assert.deepEqual(owned.words,expected);else assert.equal(Object.hasOwn(owned,'words'),false);
 assert.equal(owned.count,count);assert.equal(owned.byteLength,count*4);assert.equal(owned.finite,true);
 if(owned.words)assert.notStrictEqual(owned.words.buffer,bits.buffer);assert.notStrictEqual(packed.words.buffer,bits.buffer);
 bits.fill(0x7f800000);assert.deepEqual(packed.words,expected);if(owned.words)assert.deepEqual(owned.words,expected);
 assert(Array.isArray(owned.javascript));assert.strictEqual(owned.javascript,owned.javascript);
 assert.deepEqual(pack(owned.javascript,type).words,expected);assert.deepEqual(pack(native.javascript,type).words,expected);
 if(count){assert.throws(()=>pack(value,type),/f32 input overflow/);assert.throws(()=>capture(value,type),/f32 input overflow/);}
}
for(const bits of [0x7f800000,0xff800000,0x7fc12345,0x7fa12345]){
 const value=new Float32Array(Uint32Array.of(bits).buffer);
 assert.throws(()=>pack(value,type),/f32 input overflow/);
 assert.throws(()=>captureTransport(value,type),/f32 input overflow/);assert.throws(()=>capture(value,type),/f32 input overflow/);
 const tagged=capture([{F32Bits:bits}],type);assert.equal(tagged.finite,false);
 assert.equal(Object.hasOwn(tagged,'words'),false);assert.deepEqual(pack(tagged.javascript,type).words,Uint32Array.of(bits));
}
// The raw copy must not widen the former Number/F32Bits input domain.
assert.throws(()=>pack(new Float64Array([Number.MAX_VALUE]),type),/overflow/);
assert.throws(()=>capture(new Float64Array([Number.MAX_VALUE]),type),/overflow/);
const mutable={kind:'list',element:{kind:'f32'}};
assert.equal(Object.hasOwn(capture(new Float32Array([1]),mutable),'words'),false);
mutable.element.kind='bool';assert.deepEqual(capture([true],mutable).javascript,[true]);
const exotic=new Float32Array([1,2]);let iteratorReads=0;
Object.defineProperty(exotic,Symbol.iterator,{get(){iteratorReads++;return function*(){yield 9;yield 8;};}});
assert.deepEqual(pack(exotic,type).words,new Uint32Array(new Float32Array([9,8]).buffer));assert.equal(iteratorReads,1);
const traversed=capture(exotic,type);assert.deepEqual(traversed.javascript,[9,8]);assert.equal(iteratorReads,2);assert.equal(Object.hasOwn(traversed,'words'),false);
const shadow=new Float32Array([1,2]);let lengths=0;
Object.defineProperty(shadow,'length',{get(){lengths++;return 2;}});
assert.deepEqual(pack(shadow,type).words,new Uint32Array(new Float32Array([1,2]).buffer));assert.equal(lengths,2);
lengths=0;assert.deepEqual(capture(shadow,type).javascript,[1,2]);assert.equal(lengths,1);
class Custom extends Float32Array{*[Symbol.iterator](){yield 7;}}
assert.deepEqual(pack(new Custom([3]),type).words,new Uint32Array(new Float32Array([7]).buffer));
assert.deepEqual(capture(new Custom([3]),type).javascript,[7]);
// Prototype shape alone does not authenticate a Float32Array's internal brand.
const disguised=new Float64Array(128);disguised[0]=1/3;Object.setPrototypeOf(disguised,Float32Array.prototype);
const rounded=captureTransport(disguised,type);assert.equal(Object.hasOwn(rounded,'words'),false);assert.equal(rounded.javascript[0],Math.fround(1/3));
const bigints=new BigUint64Array(128);Object.setPrototypeOf(bigints,Float32Array.prototype);
assert.throws(()=>pack(bigints,type),/invalid F32Bits/);assert.throws(()=>capture(bigints,type),/invalid F32Bits/);
assert.throws(()=>captureTransport(bigints,type),/invalid F32Bits/);
const detached=new Float32Array(2);structuredClone(detached.buffer,{transfer:[detached.buffer]});
assert.throws(()=>pack(detached,type),TypeError);assert.throws(()=>capture(detached,type),TypeError);
const resizable=new ArrayBuffer(16,{maxByteLength:32});
if(typeof resizable.resize==='function'){
 const outOfBounds=new Float32Array(resizable,8,2);resizable.resize(4);
 assert.throws(()=>pack(outOfBounds,type),TypeError);assert.throws(()=>capture(outOfBounds,type),TypeError);
}
if(typeof SharedArrayBuffer!=='undefined'){
 const shared=new Float32Array(new SharedArrayBuffer(128*4));shared.set([1,-0]);const saved=captureTransport(shared,type);shared.fill(8);
 assert.deepEqual(saved.javascript.slice(0,2),[1,-0]);assert.notStrictEqual(saved.words.buffer,shared.buffer);
}
// A changed shared iterator retains the old traversal instead of bypassing it.
const prototype=Object.getPrototypeOf(Float32Array.prototype),descriptor=Object.getOwnPropertyDescriptor(prototype,Symbol.iterator);
try{
 Object.defineProperty(prototype,Symbol.iterator,{...descriptor,value:function*(){yield 9;yield 8;}});
 const result=pack(new Float32Array([1,2]),type);assert.equal(result.words[0],0x41100000);assert.equal(result.words[1],0x41000000);
 assert.deepEqual(capture(new Float32Array([1,2]),type).javascript,[9,8]);
}finally{Object.defineProperty(prototype,Symbol.iterator,descriptor);}
console.log('PASS finite typed ingress, lazy native capture, fresh checks, exact snapshots, aliases and traversal fallback');
