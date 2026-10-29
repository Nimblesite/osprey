import {chromium} from '../../../../../website/node_modules/playwright/index.mjs';
import {mkdir,rename} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
const output=path.resolve('../video');await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true});
const context=await browser.newContext({viewport:{width:1440,height:1080},deviceScaleFactor:1,recordVideo:{dir:output,size:{width:1440,height:1080}}});
const page=await context.newPage();
try{
 await page.goto(pathToFileURL(path.resolve('index.html')).href);
 await page.evaluate(()=>document.fonts.ready);
 await page.getByRole('button',{name:'Play walkthrough'}).click();
 await page.waitForTimeout(56000);
 const video=page.video();await context.close();await rename(await video.path(),path.join(output,'walkthrough.webm'));
 console.log('Recorded 9 chapters from the real standalone demo.');
}finally{await browser.close();}
