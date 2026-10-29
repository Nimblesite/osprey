import {useRef} from 'react';
import {FloppyDisk,FolderOpen} from '@phosphor-icons/react';
import {decodeWorkspace,saveText} from './workspaceIO';
export function WorkspaceFiles({state,dispatch}) {
  const file=useRef(null);
  const save=()=>{saveText((state.freeform ? state.title.trim().replace(/[^a-zA-Z0-9_-]+/g,'-').replace(/^-|-$/g,'').toLowerCase() || 'untitled' : 'conjunction')+'.workbench.json',JSON.stringify({format:'proof-workbench.design.v1',verification:'unchecked',state},null,2),'application/json');dispatch({value:{toast:'Workspace saved: proof, strokes and working notes. Still unchecked.',toastKind:'success'}});};
  const open=async event=>{try{const chosen=event.target.files[0];if(!chosen)return;if(chosen.size>10000000)throw new Error('Choose a workspace smaller than 10 MB.');const next=decodeWorkspace(await chosen.text());dispatch({value:{...next,toast:'Workspace restored. Verification remains unchecked.',toastKind:'success'}});}catch(error){dispatch({value:{toast:`Could not open workspace: ${error.message}`,toastKind:'error'}});}event.target.value='';};
  return <><input ref={file} type="file" accept=".json" aria-label="Open saved workspace" hidden onChange={open}/><button aria-label="Open saved workspace" title="Open saved workspace" onClick={()=>file.current.click()}><FolderOpen/></button><button aria-label="Save" title="Save workspace" className="save-button" onClick={save}><FloppyDisk/><span>Save</span></button></>;
}
