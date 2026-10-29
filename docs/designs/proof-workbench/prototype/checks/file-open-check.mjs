import {chromium} from '../../../../../website/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
import {test,before,after} from 'node:test';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
let browser;
before(async()=>{browser=await chromium.launch({headless:true});});
after(async()=>{await browser.close();});
const open=async(width=1488,query='')=>{const page=await browser.newPage({viewport:{width,height:1058}});await page.goto(pathToFileURL(path.resolve('index.html')).href+query);await page.evaluate(()=>document.fonts.ready);return page;};
test('direct file opening renders the complete workbench',async()=>{
 const page=await open();
 try{assert.equal(await page.locator('h1').count(),1);assert.equal(await page.locator('canvas').count(),1);}finally{await page.close();}
});
test('mobile import errors stay visible and announced until dismissed',async()=>{
 const page=await open(390);
 try{await page.locator('input[type=file]').setInputFiles({name:'invalid.json',mimeType:'application/json',buffer:Buffer.from('{"format":"unknown"}')});
 await page.waitForTimeout(5500);
 const error=page.getByRole('alert');assert.equal(await error.isVisible(),true);assert.match(await error.textContent(),/Could not open workspace/);
 const box=await error.boundingBox();assert.ok(box.y>=0&&box.y+box.height<=1058);
 }finally{await page.close();}
});
test('opening Source on a phone brings the panel and keyboard focus into view',async()=>{
 const page=await open(390,'?scene=5');
 try{await page.locator('.workspace-tools button').nth(2).click();await page.waitForTimeout(350);
 const panel=page.locator('.source-panel'), box=await panel.boundingBox();
 assert.ok(box.y>=0&&box.y<250,`Source starts at ${box.y}px`);assert.equal(await panel.evaluate(el=>el.contains(document.activeElement)),true);
 }finally{await page.close();}
});
test('clear confirmation contains keyboard focus and Escape returns to its trigger',async()=>{
 const page=await open();
 try{await page.getByRole('button',{name:'Clear board',exact:true}).click();const dialog=page.getByRole('alertdialog');
 assert.equal(await dialog.evaluate(el=>el.contains(document.activeElement)),true);
 for(let i=0;i<4;i++){await page.keyboard.press('Tab');assert.equal(await dialog.evaluate(el=>el.contains(document.activeElement)),true);}
 await page.keyboard.press('Escape');assert.equal(await dialog.count(),0);
 assert.equal(await page.getByRole('button',{name:'Clear board',exact:true}).evaluate(el=>el===document.activeElement),true);
 }finally{await page.close();}
});
test('notation inserts at the writing caret instead of appending to the solution',async()=>{
 const page=await open();
 try{const editor=page.getByRole('textbox',{name:'Write your solution'});await editor.fill('p  q');await editor.press('Home');await editor.press('ArrowRight');await editor.press('ArrowRight');
 await page.getByRole('button',{name:'Notation',exact:true}).click();await page.getByRole('button',{name:'∧',exact:true}).click();
 assert.equal(await editor.inputValue(),'p ∧ q');assert.equal(await editor.evaluate(el=>el===document.activeElement),true);
 }finally{await page.close();}
});
test('walkthrough receives keyboard focus and Escape restores the workspace',async()=>{
 const page=await open();
 try{await page.getByRole('textbox',{name:'Solution title'}).fill('Preserve my work');await page.getByRole('button',{name:'Play walkthrough',exact:true}).click();
 assert.equal(await page.locator('.tour').evaluate(el=>el.contains(document.activeElement)),true);
 await page.keyboard.press('Escape');assert.equal(await page.locator('.tour').count(),0);
 assert.equal(await page.getByRole('textbox',{name:'Solution title'}).inputValue(),'Preserve my work');
 }finally{await page.close();}
});
test('unsupported sentences are never presented as disproved mathematics',async()=>{
 const page=await open(1488,'?scene=3');
 try{await page.locator('#step-entry').fill('Therefore q, since hq.');await page.getByRole('button',{name:'Add proof step'}).click();
 assert.equal(await page.getByRole('alert').count(),0);
 assert.match(await page.getByRole('status').textContent(),/outside this example/i);
 assert.equal(await page.locator('.comparison').count(),0);assert.equal(await page.locator('#step-entry').getAttribute('aria-invalid'),'false');
 assert.equal(await page.locator('#step-entry').inputValue(),'Therefore q, since hq.');
 }finally{await page.close();}
});
test('the board status sits outside the drawing surface on tablet',async()=>{
 const page=await open(834,'?scene=0');
 try{const viewport=await page.locator('.board-viewport').boundingBox(), status=await page.locator('.board-caption').boundingBox();
 assert.ok(status.y>=viewport.y+viewport.height,`Status overlaps drawing surface by ${viewport.y+viewport.height-status.y}px`);
 }finally{await page.close();}
});
