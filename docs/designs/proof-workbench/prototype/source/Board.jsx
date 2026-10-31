import {useEffect,useRef,useState} from 'react';
import {Modal} from './Modal';
import chalkStudy from '../assets/chalk-study.png';
import {PencilSimple,Hand,ArrowCounterClockwise,Trash,ArrowsOut,Minus,Plus,NotePencil} from '@phosphor-icons/react';
const WIDTH=1536, HEIGHT=1024;
const point = (event,canvas) => { const rect=canvas.getBoundingClientRect(); return [(event.clientX-rect.left)/rect.width*WIDTH,(event.clientY-rect.top)/rect.height*HEIGHT]; };
function drawPath(context,path) {
  if (!path.length) return;
  context.strokeStyle='#EEEDE5'; context.fillStyle='#EEEDE5'; context.lineWidth=3; context.lineCap='round'; context.lineJoin='round'; context.globalAlpha=.88;
  context.beginPath(); context.moveTo(...path[0]); path.slice(1).forEach(p => context.lineTo(...p)); context.stroke();
  if (path.length===1) { context.beginPath(); context.arc(...path[0],1.5,0,Math.PI*2); context.fill(); }
}
export function Board({state,dispatch,focus,setFocus}) {
  const canvas=useRef(null), viewport=useRef(null), active=useRef(null), origin=useRef(null);
  const [fit,setFit]=useState(1);
  const {paths,notes,view}=state;
  const setPaths=value=>dispatch({value:{paths:typeof value==='function'?value(paths):value}}), setNotes=notes=>dispatch({value:{notes}}), setView=view=>dispatch({value:{view}});
  const [tool,setTool]=useState('pencil'), [confirm,setConfirm]=useState(false), [noteOpen,setNoteOpen]=useState(false);
  const repaint=() => { const ctx=canvas.current.getContext('2d'); ctx.clearRect(0,0,WIDTH,HEIGHT); paths.forEach(path => drawPath(ctx,path)); };
  useEffect(repaint,[paths]);
  useEffect(()=>{const observer=new ResizeObserver(([entry])=>setFit(Math.min(entry.contentRect.width/WIDTH,entry.contentRect.height/HEIGHT)));observer.observe(viewport.current);return()=>observer.disconnect();},[]);
  const down=event => { event.currentTarget.setPointerCapture(event.pointerId); if(tool==='hand') origin.current={x:event.clientX-view.x,y:event.clientY-view.y}; else active.current=[point(event,canvas.current)]; };
  const move=event => { if(origin.current) setView({...view,x:event.clientX-origin.current.x,y:event.clientY-origin.current.y}); else if(active.current) {active.current.push(point(event,canvas.current)); repaint(); drawPath(canvas.current.getContext('2d'),active.current);} };
  const up=() => { if(active.current) {const path=active.current;setPaths(previous => [...previous,path]);} active.current=null; origin.current=null; };
  const fitWorking=()=>{const rect=viewport.current.getBoundingClientRect(), scale=Math.min(rect.width/WIDTH,rect.height/HEIGHT);setView({zoom:1,scale,x:(rect.width-WIDTH*scale)/2,y:0});};
  const clear=() => { setPaths([]); dispatch({value:{sample:false}}); setConfirm(false); };
  return <section className={`board ${focus?'board-focus':''}`} aria-label="Mathematical working board"><BoardToolbar tool={tool} setTool={setTool} undo={() => setPaths(paths.slice(0,-1))} canUndo={paths.length>0} clear={() => setConfirm(true)} focus={focus} setFocus={setFocus} noteOpen={noteOpen} setNoteOpen={setNoteOpen}/><div className="board-viewport" ref={viewport}><div className="board-world" style={{transform:`translate(${view.x}px,${view.y}px) scale(${view.scale??view.zoom*fit})`}}>{state.sample && <img draggable="false" src={chalkStudy} alt="Sample sketch: p and q split from their conjunction and recombine as q and p."/>}<canvas ref={canvas} width={WIDTH} height={HEIGHT} aria-label="Draw your mathematical working" onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} style={{cursor:tool==='hand'?'grab':'crosshair'}}/></div>{!state.sample && paths.length===0 && <div className="empty-board"><span className="eyebrow">Your working board</span><h2>Start with an idea.</h2><p>Draw here. Write your argument alongside.</p></div>}{noteOpen && <label className="working-notes">Working notes<textarea value={notes} onChange={event=>setNotes(event.target.value)} placeholder="Keep the reasoning that does not fit in a diagram…"/></label>}</div><div className="board-caption"><span>{state.sample?'Example sketch · draw alongside it':'Your working'} · not checked</span><div><button aria-label="Fit working" onClick={fitWorking}>Fit</button><button aria-label="Zoom out" onClick={()=>setView({...view,scale:undefined,zoom:Math.max(.1,(view.scale??view.zoom*fit)/fit-.2)})}><Minus/></button><button onClick={()=>setView({zoom:1,x:0,y:0})}>{Math.round((view.scale??view.zoom*fit)/fit*100)}%</button><button aria-label="Zoom in" onClick={()=>setView({...view,scale:undefined,zoom:Math.min(2,(view.scale??view.zoom*fit)/fit+.2)})}><Plus/></button></div></div>{confirm && <Modal className="board-confirm" role="alertdialog" label="Clear the board" onClose={()=>setConfirm(false)}><h3>Make room for a new idea?</h3><p>This clears the sketch and your strokes. Your proof and notes stay.</p><button autoFocus onClick={()=>setConfirm(false)}>Keep working</button><button className="danger-button" onClick={clear}>Clear board</button></Modal>}</section>;
}
function BoardToolbar({tool,setTool,undo,canUndo,clear,focus,setFocus,noteOpen,setNoteOpen}) {
  return <div className="board-toolbar"><div className="board-title">Working board<span>Think freely.</span></div><div className="board-tools"><button title="Draw" aria-label="Draw" aria-pressed={tool==='pencil'} onClick={()=>setTool('pencil')}><PencilSimple/></button><button title="Pan board" aria-label="Pan board" aria-pressed={tool==='hand'} onClick={()=>setTool('hand')}><Hand/></button><span className="tool-divider"/><button title="Undo stroke" aria-label="Undo stroke" disabled={!canUndo} onClick={undo}><ArrowCounterClockwise/></button><button title="Working notes" aria-label="Working notes" aria-pressed={noteOpen} onClick={()=>setNoteOpen(!noteOpen)}><NotePencil/></button><button title="Clear board" aria-label="Clear board" onClick={clear}><Trash/></button><button title="Focus board" aria-label="Focus board" aria-pressed={focus} onClick={()=>setFocus(!focus)}><ArrowsOut/></button></div></div>;
}
