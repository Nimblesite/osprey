import {useEffect,useRef,useState} from 'react';
import {Play,Pause,ArrowLeft,ArrowRight,X} from '@phosphor-icons/react';
import {Modal} from './Modal';
import {chapters} from './model';
export const SCENE_MS=6000;
export function useTour(state,dispatch) {
  const [playing,setPlaying]=useState(false), [position,setPosition]=useState(0), [visible,setVisible]=useState(false);
  const backup=useRef(null);
  const last=useRef(-1), index=Math.min(chapters.length-1,Math.floor(position/SCENE_MS));
  useEffect(()=>{ if(visible && last.current!==index){dispatch({type:'scene',index});last.current=index;} },[index,visible,dispatch]);
  useEffect(()=>{ if(!playing)return; const interval=setInterval(()=>setPosition(value=>Math.min(value+100,chapters.length*SCENE_MS-1)),100); return ()=>clearInterval(interval); },[playing]);
  useEffect(()=>{if(position>=chapters.length*SCENE_MS-1)setPlaying(false);},[position]);
  const start=()=>{if(!visible)backup.current=state;last.current=-1;setPosition(0);setVisible(true);setPlaying(true);};
  const jump=value=>{setPosition(Math.max(0,Math.min(chapters.length-1,value))*SCENE_MS);last.current=-1;};
  return {playing,position,visible,index,start,jump,toggle:()=>setPlaying(!playing),close:()=>{setPlaying(false);setVisible(false);if(backup.current)dispatch({value:backup.current});}};
}
export function Tour({tour}) {
  if(!tour.visible)return null;
  return <Modal className="tour" label="Animated design walkthrough" onClose={tour.close}><div className="tour-copy" key={tour.index}><span className="eyebrow">{String(tour.index+1).padStart(2,'0')} / {String(chapters.length).padStart(2,'0')} · A working session</span><h2>{chapters[tour.index].title}</h2><p>{chapters[tour.index].copy}</p></div><div className="tour-controls"><button aria-label="Previous chapter" onClick={()=>tour.jump(tour.index-1)}><ArrowLeft/></button><button autoFocus className="tour-play" aria-label={tour.playing?'Pause walkthrough':'Play walkthrough'} onClick={tour.toggle}>{tour.playing?<Pause weight="fill"/>:<Play weight="fill"/>}</button><button aria-label="Next chapter" onClick={()=>tour.jump(tour.index+1)}><ArrowRight/></button><button aria-label="Close walkthrough" onClick={tour.close}><X/></button></div><div className="tour-track"><div style={{width:`${tour.position/(chapters.length*SCENE_MS)*100}%`}}/></div></Modal>;
}
