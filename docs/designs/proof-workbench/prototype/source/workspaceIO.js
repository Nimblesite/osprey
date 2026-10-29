import {initial} from './model.js';
export function decodeWorkspace(text) {
  const data=JSON.parse(text), state=data.state;
  if(data.format!=='proof-workbench.design.v1'||!state) throw new Error('Choose a Proof Workbench design workspace.');
  const validPoint=p=>Array.isArray(p)&&p.length===2&&p.every(n=>Number.isFinite(n)&&Math.abs(n)<100000);
  if(!Array.isArray(state.paths)||state.paths.length>10000||!state.paths.every(path=>Array.isArray(path)&&path.length<100000&&path.every(validPoint))) throw new Error('The board contains invalid stroke data.');
  if(!Number.isInteger(state.count)||state.count<0||state.count>5||typeof state.notes!=='string'||typeof state.input!=='string'||typeof state.sample!=='boolean') throw new Error('The document contains invalid working data.');
  const view=state.view??initial.view;
  if(!Number.isFinite(view.zoom)||view.zoom<=0||view.zoom>10||![view.x,view.y].every(n=>Number.isFinite(n)&&Math.abs(n)<100000)||(view.scale!==undefined&&(!Number.isFinite(view.scale)||view.scale<=0||view.scale>10))) throw new Error('The board contains an invalid view.');
  return {...initial,view,freeform:state.freeform!==false,title:typeof state.title==='string'?state.title:'Untitled solution',working:typeof state.working==='string'?state.working:'',paths:state.paths,notes:state.notes,input:state.input,sample:state.sample,count:state.count,selected:state.count,complete:state.count===5};
}
export function saveText(filename,text,type='text/plain') {
  const url=URL.createObjectURL(new Blob([text],{type})), link=document.createElement('a');
  link.href=url; link.download=filename; link.click(); setTimeout(()=>URL.revokeObjectURL(url),1000);
}
