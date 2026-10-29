import {useEffect,useReducer,useState} from 'react';
import {ArrowRight} from '@phosphor-icons/react';
import '@fontsource/stix-two-text/400.css';
import '@fontsource/stix-two-text/400-italic.css';
import '@fontsource/stix-two-text/600.css';
import '@fontsource/stix-two-math';
import '@fontsource/geist/400.css';
import '@fontsource/geist/500.css';
import {initial,reducer} from './model';
import {Toolbar} from './Toolbar';
import {Proof,Goal,Panel} from './Proof';
import {Board} from './Board';
import {Tour,useTour} from './Demo';
import './styles.css';
const startState=()=>{const scene=new URLSearchParams(location.search).get('scene');return scene===null?initial:reducer(initial,{type:'scene',index:Math.max(0,Math.min(8,Number(scene)||0))});};
export function App() {
  const [state,dispatch]=useReducer(reducer,null,startState), [focus,setFocus]=useState(false);
  const tour=useTour(state,dispatch);
  useEffect(()=>{if(!state.toast||state.toastKind==='error')return;const timer=setTimeout(()=>dispatch({value:{toast:''}}),4500);return()=>clearTimeout(timer);},[state.toast,state.toastKind]);
  useEffect(()=>{setFocus(false);},[state.mode,tour.index]);
  return <div className={`app ${state.theme} mode-${state.mode} ${focus?'focused':''} ${tour.visible?'touring':''} ${state.panel==='source'?'with-source':''} ${state.paperBoard?'paper-board-open':''}`}><div className="workspace-shell" inert={tour.visible}><Toolbar state={state} dispatch={dispatch} tour={tour}/><main className="workspace"><div className="document"><Proof state={state} dispatch={dispatch}/><Goal state={state}/></div><div className="board-region">{state.mode==='paper' && <button className="board-disclosure" aria-expanded={Boolean(state.paperBoard)} onClick={()=>dispatch({value:{paperBoard:!state.paperBoard}})}>{state.paperBoard?'Hide working board':'Show working board'}</button>}<Board state={state} dispatch={dispatch} focus={focus} setFocus={setFocus}/>{!state.freeform && <div className="board-to-proof"><span>Example sketch · transcription preview</span><button onClick={()=>dispatch({value:{panel:'recognition'}})}>Preview sample transcription <ArrowRight/></button></div>}</div><Panel state={state} dispatch={dispatch}/></main><footer className="app-footer"><span>Interactive design prototype · example checks only · no proof kernel connected</span><span>Paper & Slate / 01</span></footer></div><Tour tour={tour}/></div>;
}
