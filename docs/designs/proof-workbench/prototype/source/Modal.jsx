import {useEffect,useRef} from 'react';
export function Modal({className,label,role='dialog',onClose,children}) {
 const ref=useRef(null), close=useRef(onClose);close.current=onClose;
 useEffect(()=>{const dialog=ref.current, trigger=document.activeElement;dialog.showModal();return()=>{dialog.close();trigger?.focus({preventScroll:true});};},[]);
 const containFocus=event=>{if(event.key!=='Tab')return;const buttons=[...ref.current.querySelectorAll('button:not(:disabled)')], first=buttons[0], last=buttons.at(-1);if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}};
 return <dialog ref={ref} className={className} aria-label={label} role={role} onKeyDown={containFocus} onCancel={event=>{event.preventDefault();close.current();}}>{children}</dialog>;
}
