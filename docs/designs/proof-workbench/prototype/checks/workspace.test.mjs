import test from 'node:test';
import assert from 'node:assert/strict';
import {decodeWorkspace} from '../source/workspaceIO.js';
import {initial,reducer,steps} from '../source/model.js';
test('workspace import retains actual working and cannot restore a checked verdict',()=>{
 const state={...initial,count:4,notes:'Try the converse.',paths:[[[10,20],[25,35]]],input:'From hp.',sample:false,complete:true,verification:'checked'};
 const restored=decodeWorkspace(JSON.stringify({format:'proof-workbench.design.v1',state}));
 assert.deepEqual(restored.paths,state.paths);assert.equal(restored.notes,state.notes);assert.equal(restored.input,state.input);assert.equal(restored.sample,false);assert.equal(restored.complete,false);assert.equal(restored.verification,undefined);
});
test('unknown files and malformed strokes are refused',()=>{
 for(const data of [{format:'other',state:initial},{format:'proof-workbench.design.v1',state:{...initial,paths:[[[null,20]]]}}]) assert.throws(()=>decodeWorkspace(JSON.stringify(data)));
});
test('a wrong step keeps the existing argument and reports a mismatch',()=>{
 const result=reducer({...initial,input:'From hp.'},{type:'step'});
 assert.equal(result.count,3);assert.match(result.error,/current goal is q/);assert.equal(result.complete,false);
});
test('all fixture steps close draft goals without inventing a checker verdict',()=>{
 const final=steps.reduce((state,input)=>reducer({...state,input},{type:'step'}),reducer(initial,{type:'reset'}));
 assert.equal(final.count,5);assert.equal(final.complete,true);assert.equal(final.error,'');assert.equal(final.verification,undefined);
});
test('restarting the guided proof preserves the board and working notes',()=>{
 const working={...initial,paths:[[[5,6],[7,8]]],notes:'Keep this calculation'};
 const result=reducer(working,{type:'reset'});
 assert.deepEqual(result.paths,working.paths);assert.equal(result.notes,working.notes);
});
test('reopening working retains a valid board view and rejects broken transforms',()=>{
 const view={zoom:.75,scale:.5,x:-220,y:42}, state={...initial,view};
 assert.deepEqual(decodeWorkspace(JSON.stringify({format:'proof-workbench.design.v1',state})).view,view);
 for(const bad of [{zoom:0,x:0,y:0},{zoom:1,x:'far',y:0},{zoom:1,x:0,y:0,scale:-1}]) assert.throws(()=>decodeWorkspace(JSON.stringify({format:'proof-workbench.design.v1',state:{...state,view:bad}})));
});
