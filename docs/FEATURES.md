# Folitecta feature matrix

Generated from the command registry for version 0.1.0. Every command of the full feature set is present in the user interface; this table records which ones perform their function today.

| Status | Count |
|---|---:|
| ✅ available | 1436 |
| 🟡 partial | 62 |
| ⬜ planned | 483 |
| total | 1981 |

Legend: *available* performs the complete operation; *partial* performs a reduced version (documented in the UI); *planned* is listed in menus and ribbon but shows a roadmap notice when invoked. A submenu counts as available when all of its commands are available, and as partial when only some of them work.

## Ribbon

### Edit

**Tools**

| Command | Id | Status |
|---|---|---|
| Continuous Zoom | `tool.zoomDyn` | ✅ available |
| Hand Tool | `tool.hand` | ✅ available |
| ↳ Selection Text Images | `tool.hand.selTextImages` | ✅ available |
| Loupe Tool | `tool.loupe` | ✅ available |
| ↳ Track Mouse | `tool.loupe.trackMouse` | ✅ available |
| Other Tools | `comment.otherToolsGroup` | ⬜ planned |
| Other Tools | `otherToolsGroup` | ⬜ planned |
| Pan and Zoom Tool | `tool.panZoom` | ✅ available |
| Place Annotation | `tool.selAnnotPlace` | ⬜ planned |
| Select Text | `tool.selectText` | ✅ available |
| ↳ Copy as Rich | `tool.selectText.copyAsRich` | ✅ available |
| Snapshot Tool | `tool.snapshot` | ✅ available |
| Tools | `tools` | 🟡 partial |
| ↳ Basic Tools | `tools.basic` | ⬜ planned |
| ↳ Comment and Markup Tools | `tools.markup` | ⬜ planned |
| ↳ Content Editing Tools | `tools.editContent` | ⬜ planned |
| ↳ Link Tools | `tools.editLinks` | ⬜ planned |
| ↳ Measuring Tools | `tools.measure` | ⬜ planned |
| ↳ Media Tools | `tools.media` | ✅ available |
| ↳ Zoom Tools | `tools.zoom` | ⬜ planned |
| Zoom Tool | `tool.zoom` | ✅ available |
| Zoom Tools | `zoomToolsGroup` | ⬜ planned |

**Text**

| Command | Id | Status |
|---|---|---|
| Delete Text | `pageText.delete` | ✅ available |
| Edit Text | `tool.editText` | ✅ available |
| Find and Replace… | `findAndRepl` | ✅ available |
| Find… | `find` | ✅ available |
| ↳ Annotate Results | `find.annotateResults` | ✅ available |
| ↳ Edit… | `find.edit` | ✅ available |
| Insert Text | `pageText.insert` | ✅ available |
| Replace Text | `pageText.replace` | ✅ available |

**Clipboard**

| Command | Id | Status |
|---|---|---|
| Copy | `copy` | ✅ available |
| Cut | `cut` | ✅ available |
| Delete | `delete` | ✅ available |
| Paste | `paste` | ✅ available |
| Paste Options | `pasteSpecial` | ✅ available |
| Redo | `redo` | ✅ available |
| Select All | `selectAll` | ✅ available |
| Undo | `undo` | ✅ available |

**Objects**

| Command | Id | Status |
|---|---|---|
| Add Barcode | `tool.selBarcodePlace` | ✅ available |
| Add Image… | `document.addImage` | ✅ available |
| Add Image… | `tool.selImagePlace` | ✅ available |
| Add Text | `tool.addText` | ✅ available |
| Edit Content | `tool.editContent` | 🟡 partial |
| ↳ All | `tool.editContent.all` | ⬜ planned |
| ↳ Images | `tool.editContent.images` | ✅ available |
| ↳ Shades | `tool.editContent.shades` | ⬜ planned |
| ↳ Shapes | `tool.editContent.shapes` | ⬜ planned |
| ↳ Text | `tool.editContent.text` | ✅ available |

**Comment**

| Command | Id | Status |
|---|---|---|
| Add Sticky Note | `addStickyNote` | ✅ available |
| Arrow | `tool.annot.arrow` | ✅ available |
| Ellipse | `tool.annot.circle` | ✅ available |
| Highlight Text | `tool.annot.highlight` | ✅ available |
| Pencil | `tool.annot.pencil` | ✅ available |
| Rectangle | `tool.annot.square` | ✅ available |
| Stamp | `tool.annot.stamp` | ✅ available |
| Sticky Note | `tool.annot.stickyNote` | ✅ available |
| Strikeout Text | `tool.annot.strikeout` | ✅ available |
| Text Box | `tool.annot.textBox` | ✅ available |
| Typewriter | `tool.annot.typeWriter` | ✅ available |
| Underline Text | `tool.annot.underline` | ✅ available |

**View**

| Command | Id | Status |
|---|---|---|
| Actual Size | `zoom.level.actual` | ✅ available |
| First Page | `view.goto.firstPage` | ✅ available |
| Fit Height | `zoom.level.fitHeight` | ✅ available |
| Fit Page | `zoom.level.fitPage` | ✅ available |
| Fit Visible | `zoom.level.fitVisible` | ✅ available |
| Fit Width | `zoom.level.fitWidth` | ✅ available |
| Last Page | `view.goto.lastPage` | ✅ available |
| Next Page | `view.goto.nextPage` | ✅ available |
| Page Number | `view.goto.pagesView` | ✅ available |
| Previous Page | `view.goto.prevPage` | ✅ available |
| Zoom In | `zoomIn` | ✅ available |
| Zoom Level | `zoom.combo` | ✅ available |
| Zoom Out | `zoomOut` | ✅ available |
| Zoom To… | `zoom.to.level` | ✅ available |

**Links**

| Command | Id | Status |
|---|---|---|
| Add Link… | `addLink` | ✅ available |
| ↳ Add 'Go To' Link… | `addLink.goto` | ✅ available |
| ↳ Add 'Open File' Link… | `addLink.launch` | ✅ available |
| ↳ Add Custom Link… | `addLink.custom` | ✅ available |
| ↳ Add Web Link… | `addLink.uri` | ✅ available |
| Add/Edit Links | `tool.annot.editLink` | ✅ available |
| Create Web Links from Barcodes | `createWebLinksFromBarcodes` | ✅ available |
| Create Web Links from URLs | `createWebLinksFromText` | ✅ available |
| Remove All Web Links | `removeAllWebLinks` | ✅ available |

**Protect**

| Command | Id | Status |
|---|---|---|
| Mark for Redaction | `tool.annot.redaction` | ✅ available |
| Place Signature or Initials | `tool.sign.and.initials` | ✅ available |
| Security Properties… | `document.security` | ✅ available |
| Sign Document… | `document.signatures.sign` | ✅ available |

### View

**Tools**

| Command | Id | Status |
|---|---|---|
| Loupe | `view.loupe` | ✅ available |
| Pan and Zoom | `view.panzoom` | ✅ available |

**Go To**

| Command | Id | Status |
|---|---|---|
| Go To | `view.goto` | ✅ available |
| ↳ First Page | `view.goto.firstPage` | ✅ available |
| ↳ Go to Page… | `view.goto.page` | ✅ available |
| ↳ Last Page | `view.goto.lastPage` | ✅ available |
| ↳ Next Document | `view.goto.nextDocument` | ✅ available |
| ↳ Next Page | `view.goto.nextPage` | ✅ available |
| ↳ Next View | `view.goto.nextView` | ✅ available |
| ↳ Page Number | `view.goto.pagesView` | ✅ available |
| ↳ Previous Document | `view.goto.prevDocument` | ✅ available |
| ↳ Previous Page | `view.goto.prevPage` | ✅ available |
| ↳ Previous View | `view.goto.prevView` | ✅ available |

**Zoom**

| Command | Id | Status |
|---|---|---|
| Zoom | `view.zoom` | ✅ available |
| Zoom | `zoom` | ✅ available |
| ↳ Level | `zoom.level` | ✅ available |
| ↳ To | `zoom.to` | ✅ available |
| ↳ Zoom Level | `zoom.combo` | ✅ available |
| Zoom Slider | `zoomSlider` | ⬜ planned |
| Zoom to Selection | `zoomSel` | ✅ available |

**Page Display**

| Command | Id | Status |
|---|---|---|
| Dark Page Mode | `view.darkPageMode` | ✅ available |
| Exit Full Screen | `view.fullScreenExit` | ✅ available |
| Full Screen | `view.fullScreen` | ✅ available |
| Grid | `view.showGrid` | ✅ available |
| Guides | `view.showGuides` | ✅ available |
| Highlight Links | `view.highlightLinks` | ✅ available |
| Invert Vertical Axis | `view.mathY` | ✅ available |
| Override Document Colors | `view.overrideDocumentColors` | ✅ available |
| Page | `view.pageNumber` | ✅ available |
| Page Layout | `view.layout` | ✅ available |
| ↳ Auto Scroll | `view.layout.autoScroll` | ✅ available |
| ↳ Continuous | `view.layout.oneColumn` | ✅ available |
| ↳ Horizontal Scrolling | `view.layout.ribbon` | ✅ available |
| ↳ Right-to-Left Page Layout | `view.layout.r2l` | ✅ available |
| ↳ Show Cover Page in Two-Page Mode | `view.layout.showCover` | ✅ available |
| ↳ Show Gaps Between Pages | `view.layout.showGaps` | ✅ available |
| ↳ Single Page | `view.layout.onePage` | ✅ available |
| ↳ Standard Layout | `view.layout.standard` | ✅ available |
| ↳ Two Pages | `view.layout.twoPages` | ✅ available |
| ↳ Two Pages Continuous | `view.layout.twoColumns` | ✅ available |
| Page Layout | `view.pageLayoutTypesGroup` | ⬜ planned |
| Page Size | `view.pageMeasureInfo` | ✅ available |
| ↳ Coordinates | `view.pageMeasureInfo.XY` | ✅ available |
| ↳ Units | `view.pageMeasureInfo.units` | ✅ available |
| ↳ Width/Height | `view.pageMeasureInfo.WH` | ✅ available |
| Page Size Popup | `view.showPageSizePopup` | ✅ available |
| Rotate View | `view.rotate` | ✅ available |
| ↳ Reset Rotation | `view.rotate.reset` | ✅ available |
| ↳ Rotate Clockwise | `view.rotate.CW` | ✅ available |
| ↳ Rotate Counterclockwise | `view.rotate.CCW` | ✅ available |
| Rotate View | `view.rotateViewGroup` | ⬜ planned |
| Rulers | `rulers` | ✅ available |
| ↳ Add New Guides | `rulers.addNewGuides` | ✅ available |
| ↳ Delete All Guides | `rulers.deleteAllGuides` | ✅ available |
| ↳ Delete Guides on Current Page | `rulers.deleteGuidesOnCurrentPage` | ✅ available |
| ↳ Duplicate Guides | `rulers.duplicateGuides` | ✅ available |
| ↳ Export Guides | `rulers.exportGuides` | ✅ available |
| ↳ Import Guides | `rulers.importGuides` | ✅ available |
| ↳ Units | `rulers.units` | ✅ available |
| Rulers | `view.showRulers` | ✅ available |
| Show | `view.show` | ⬜ planned |
| Show Page Size | `view.showPageMeasureInfo` | ✅ available |
| Snap | `view.snap` | ✅ available |
| ↳ Settings | `view.snap.settings` | ⬜ planned |
| ↳ To Grid | `view.snap.toGrid` | ✅ available |
| ↳ To Guides | `view.snap.toGuides` | ✅ available |
| ↳ To Objects | `view.snap.toObjects` | ⬜ planned |
| Snap | `view.snapGroup` | ⬜ planned |
| Thin Lines | `view.thinLines` | ⬜ planned |
| Transparency Grid | `view.showTranspGrid` | ✅ available |

**Window**

| Command | Id | Status |
|---|---|---|
| 3D Model Tree | `view.3DModelTree` | ⬜ planned |
| Accessibility Checker | `view.accChecker` | ✅ available |
| Accessibility Report | `view.accReport` | ✅ available |
| Articles | `view.articles` | ⬜ planned |
| Attachments | `view.attachments` | ✅ available |
| Bookmarks | `view.bookmarks` | ✅ available |
| Classic UI | `view.switchToClassicUI` | ✅ available |
| Comment Styles Palette | `view.commentStyles` | ✅ available |
| Comments | `view.comments` | ✅ available |
| Content | `view.content` | ✅ available |
| Content Editing | `view.toolbar.contentEditing` | ✅ available |
| Customize Toolbars… | `view.toolbar.customize` | ✅ available |
| Document Info Bar | `view.docInfoBar` | ✅ available |
| Document Options | `view.toolbar.docOptions` | ✅ available |
| Document Recovery | `view.recovery` | ✅ available |
| Document Tabs | `view.docTabsGroup` | ⬜ planned |
| Drawing | `view.toolbar.drawing` | ✅ available |
| Fields | `view.fields` | ✅ available |
| File | `view.toolbar.file` | ✅ available |
| Form | `view.toolbar.form` | ✅ available |
| History | `view.history` | ✅ available |
| JavaScript Options… | `view.javaScriptOpts` | ✅ available |
| Launch Application | `view.toolbar.launchApp` | ✅ available |
| Layers | `view.layers` | ✅ available |
| Links | `view.links` | ✅ available |
| Lock All Cmd Bars | `view.lockAllCmdBars` | ⬜ planned |
| Lock All Cmd Panes | `view.lockAllCmdPanes` | ⬜ planned |
| Lock Unlock All Cmd Bars | `view.lockUnlockAllCmdBars` | ⬜ planned |
| Lock Unlock All Cmd Panes | `view.lockUnlockAllCmdPanes` | ⬜ planned |
| Measuring | `view.toolbar.measuring` | ✅ available |
| Menu Bar | `view.toolbar.menu` | ✅ available |
| Mouse-Optimized UI | `view.switchToMouseUI` | ✅ available |
| Named Destinations | `view.destinations` | ✅ available |
| Navigation | `view.toolbar.navigation` | ✅ available |
| Order | `view.order` | ✅ available |
| Other Panes | `view.otherPanes` | ⬜ planned |
| Other Tools | `view.otherToolsGroup` | ⬜ planned |
| Page Layout | `view.toolbar.pageLayout` | ✅ available |
| Page Zoom | `view.toolbar.pageZoom` | ✅ available |
| Panes | `view.panesGroup` | ⬜ planned |
| Portfolio | `view.portfolio` | 🟡 partial |
| ↳ Add Files | `view.portfolio.addFiles` | ✅ available |
| ↳ Add Folder | `view.portfolio.addFolder` | ✅ available |
| ↳ Cover | `view.portfolio.cover` | ✅ available |
| ↳ Create Folder | `view.portfolio.createFolder` | ✅ available |
| ↳ Delete | `view.portfolio.delete` | ✅ available |
| ↳ Deselect All | `view.portfolio.deselectAll` | ✅ available |
| ↳ Extract Files | `view.portfolio.extractFiles` | ✅ available |
| ↳ Item Info | `view.portfolio.itemInfo` | ✅ available |
| ↳ Make Default | `view.portfolio.makeDefault` | ✅ available |
| ↳ Navigation Details | `view.portfolio.navDetails` | ✅ available |
| ↳ Navigation Layout | `view.portfolio.navLayout` | ✅ available |
| ↳ Preview | `view.portfolio.preview` | ✅ available |
| ↳ Preview File | `view.portfolio.previewFile` | ✅ available |
| ↳ Print | `view.portfolio.print` | 🟡 partial |
| ↳ Properties | `view.portfolio.properties` | ✅ available |
| ↳ Schema View | `view.portfolio.schemaView` | ✅ available |
| ↳ Select All | `view.portfolio.selectAll` | ✅ available |
| ↳ Sort by | `view.portfolio.sortBy` | ✅ available |
| ↳ View | `view.portfolio.view` | ✅ available |
| ↳ View Fields | `view.portfolio.viewFields` | ✅ available |
| Portfolio Tools | `view.portfolioTools` | ✅ available |
| Properties | `view.toolbar.properties` | ✅ available |
| Properties Pane | `view.properties` | ✅ available |
| Restore Cmd Bars Visibility | `view.restoreCmdBarsVisibility` | ✅ available |
| Ribbon / Classic UI | `view.ribbonClassicUI` | ✅ available |
| Ribbon UI | `view.switchToRibbonUI` | ✅ available |
| Rotateview | `view.toolbar.rotateview` | ✅ available |
| Search | `view.search` | ✅ available |
| Signatures | `view.signatures` | ✅ available |
| Spell Check | `view.spellCheck` | ✅ available |
| Stamps Palette | `view.stamps` | ✅ available |
| Standard | `view.toolbar.standard` | ✅ available |
| Tags | `view.tags` | ✅ available |
| Thumbnails | `view.pageThumbnails` | ✅ available |
| Toggle All Cmd Bars | `view.toggleAllCmdBars` | ✅ available |
| Toggle All Cmd Panes | `view.toggleAllCmdPanes` | ✅ available |
| Tool Names | `view.ribbonLabels` | ✅ available |
| Toolbars | `view.toolbars` | ✅ available |
| Touch-Optimized UI | `view.switchToTouchUI` | ✅ available |
| Touch/Mouse Mode | `view.touchMouseMode` | ✅ available |
| UI Options | `view.uiOptions` | ✅ available |
| UI Theme | `view.theme` | ✅ available |
| ↳ Black Theme | `view.theme.black` | ✅ available |
| ↳ Customize Theme… | `view.theme.customize` | ✅ available |
| ↳ Dark Gray Theme | `view.theme.darkGray` | ✅ available |
| ↳ Default Theme | `view.theme.default` | ✅ available |
| ↳ Light Theme | `view.theme.white` | ✅ available |
| Unlock All Cmd Bars | `view.unlockAllCmdBars` | ⬜ planned |
| Unlock All Cmd Panes | `view.unlockAllCmdPanes` | ⬜ planned |
| View | `view.toolbar.view` | ✅ available |
| Window | `window` | 🟡 partial |
| ↳ Distribute Tabs | `window.distributeTabs` | ⬜ planned |
| ↳ Horizontally | `window.distributeDocsHorz` | ⬜ planned |
| ↳ Merge All Tabs into One Group | `window.allDocsToOneTabGroup` | ⬜ planned |
| ↳ Merge All Tabs into This Window | `window.allDocsToActiveWindow` | ⬜ planned |
| ↳ Move Tab To | `window.moveTabTo` | ⬜ planned |
| ↳ Move Tabs… | `window.moveTabsTo` | ⬜ planned |
| ↳ Move to New Window | `window.activeDocToNewWindow` | ⬜ planned |
| ↳ New Blank Window | `window.newBlank` | ✅ available |
| ↳ New Document Tab | `window.new` | ✅ available |
| ↳ New Horizontal Tab Group | `window.activeDocToNewHorzTabGroup` | ⬜ planned |
| ↳ New Vertical Tab Group | `window.activeDocToNewVertTabGroup` | ⬜ planned |
| ↳ Remove Split | `window.removeSplit` | ✅ available |
| ↳ Reset Document Panes Layout | `window.resetDocPanesLayout` | ✅ available |
| ↳ Reset Main Panes Layout | `window.resetMainPanesLayout` | ✅ available |
| ↳ Sort Tabs Alphabetically | `window.sortAllDocTabsAbc` | ✅ available |
| ↳ Split | `window.split` | 🟡 partial |
| ↳ Vertically | `window.distributeDocsVert` | ⬜ planned |
| XFA Tree | `view.xfaTree` | ⬜ planned |
| Zoom | `view.toolbar.zoom` | ✅ available |

### Comment

**Tools**

| Command | Id | Status |
|---|---|---|
| Edit Comments | `tool.editAnnots` | ✅ available |

**Text**

| Command | Id | Status |
|---|---|---|
| Callout | `tool.annot.callout` | ✅ available |

**Note**

| Command | Id | Status |
|---|---|---|
| File Attachment | `tool.annot.fileAttachment` | ✅ available |
| Sound | `tool.annot.sound` | ✅ available |

**Text Markup**

| Command | Id | Status |
|---|---|---|
| Add Highlight | `textsel.addHighlight` | ✅ available |
| Add Insert Text Note | `textsel.addInsertTextNote` | ✅ available |
| Add Replace Text Note | `textsel.addReplaceTextNote` | ✅ available |
| Add Strikeout | `textsel.addStrikeout` | ✅ available |
| Add Underline | `textsel.addUnderline` | ✅ available |
| Comment | `textsel.comment` | ⬜ planned |
| Highlight | `addAnnotBySelect.highlight` | ✅ available |
| Insert Text | `tool.annot.insertText` | ✅ available |
| Redaction | `addAnnotBySelect.redaction` | ✅ available |
| Replace Text | `tool.annot.replaceText` | ✅ available |
| Strikeout | `addAnnotBySelect.strikeout` | ✅ available |
| Text Correction | `tool.textCorrectionMarkup` | ✅ available |
| Underline | `addAnnotBySelect.underline` | ✅ available |

**Drawing**

| Command | Id | Status |
|---|---|---|
| Cloud | `tool.annot.cloud` | ✅ available |
| Eraser | `tool.annot.eraser` | ✅ available |
| Free Highlight | `tool.annot.freeHighlight` | ✅ available |
| Highlight Area | `tool.annot.highlightArea` | ✅ available |
| Line | `tool.annot.line` | ✅ available |
| Polygon | `tool.annot.polygon` | ✅ available |
| Polyline | `tool.annot.polyline` | ✅ available |

**Measure**

| Command | Id | Status |
|---|---|---|
| Area | `tool.annot.area` | ✅ available |
| Calibrate Measurement | `tool.calibrate` | ✅ available |
| Distance | `tool.annot.distance` | ✅ available |
| Export Measures to CSV | `exportMeasuresToCSV` | ✅ available |
| Measure | `measure` | ✅ available |
| ↳ Decimal Separator | `measure.decimalSeparator` | ✅ available |
| ↳ Group Separator | `measure.groupSeparator` | ✅ available |
| ↳ Precision | `measure.precision` | ✅ available |
| Measure Edit | `measureEdit` | ⬜ planned |
| Measure Info | `measureInfo` | ⬜ planned |
| ↳ Copy | `measureInfo.copy` | ⬜ planned |
| Measurement Options… | `prefs.measurement` | ⬜ planned |
| Measures | `measures` | ⬜ planned |
| ↳ Manage… | `measures.manage` | ⬜ planned |
| Perimeter | `tool.annot.perimeter` | ✅ available |

**Manage Comments**

| Command | Id | Status |
|---|---|---|
| Accounting Calculator | `comments.accountingCalc` | ✅ available |
| Add Comments | `comments.newGroup` | ⬜ planned |
| Comment Styles | `annotStylesList` | ⬜ planned |
| Comment Styles | `commentStyles` | ✅ available |
| ↳ Apply Style | `commentStyles.applyStyle` | ✅ available |
| ↳ Clone… | `commentStyles.clone` | ✅ available |
| ↳ Delete | `commentStyles.delete` | ✅ available |
| ↳ Rename… | `commentStyles.rename` | ✅ available |
| ↳ Reset | `commentStyles.reset` | ✅ available |
| ↳ Reset Style | `commentStyles.resetStyle` | ✅ available |
| ↳ Show Properties | `commentStyles.showProps` | ✅ available |
| Copy | `annotStyle.copy` | ✅ available |
| Delete All Comments | `comments.deleteAllDoc` | ✅ available |
| Delete All Visible Comments | `comments.deleteAllShown` | ✅ available |
| Delete Comments | `comments.deleteGroup` | ⬜ planned |
| Delete Selected Comments | `comments.deleteSel` | ✅ available |
| Export Comments to Data File… | `comments.export` | ✅ available |
| Flatten Comments | `comments.flattenGroup` | ✅ available |
| Flatten Comments… | `comments.flatten` | ✅ available |
| Flatten Selected | `comments.flattenSel` | ✅ available |
| Hide All Comments | `comments.hideAll` | ✅ available |
| Import Comments… | `comments.import` | ✅ available |
| Next Comment | `comments.next` | ✅ available |
| Paste | `annotStyle.paste` | ✅ available |
| Paste Application Only | `annotStyle.pasteAppOnly` | ✅ available |
| Previous Comment | `comments.previous` | ✅ available |
| Show All Comments | `comments.showAll` | ✅ available |
| Show Comments | `comments.show` | ✅ available |
| ↳ By Author | `comments.show.byAuthor` | ✅ available |
| ↳ By Check Mark | `comments.show.checked` | ✅ available |
| ↳ By Status | `comments.show.status` | ✅ available |
| ↳ By Type | `comments.show.byType` | ✅ available |
| ↳ Popups | `comments.show.popups` | ✅ available |
| Show/Hide All Comments | `comments.showhideAll` | ✅ available |
| Summarize Comments… | `comments.summarize` | ✅ available |
| Tool Styles List | `toolStylesList` | ⬜ planned |

**Popup**

| Command | Id | Status |
|---|---|---|
| Hide | `annot.popup.hide` | ✅ available |
| Open | `annot.popup.open` | ✅ available |
| Reset Location | `annot.popup.resetLocation` | ✅ available |

**Collaboration**

| Command | Id | Status |
|---|---|---|
| Add Reply | `annot.addReply` | ✅ available |
| Checked | `comments.checked` | ✅ available |
| Locked | `comments.locked` | ✅ available |
| Printable | `comments.printable` | ✅ available |
| Review History | `annot.reviewHistory` | ✅ available |
| Status | `annot.status` | ✅ available |

**Media**

| Command | Id | Status |
|---|---|---|
| 3D | `3d` | ⬜ planned |
| ↳ Camera Properties | `3d.cameraProps` | ⬜ planned |
| ↳ Default View | `3d.defaultView` | ⬜ planned |
| ↳ Fly | `3d.fly` | ⬜ planned |
| ↳ Measure Tool | `3d.measureTool` | ⬜ planned |
| ↳ Pan | `3d.pan` | ⬜ planned |
| ↳ Rotate | `3d.rotate` | ⬜ planned |
| ↳ Spin | `3d.spin` | ⬜ planned |
| ↳ Walk | `3d.walk` | ⬜ planned |
| ↳ Zoom | `3d.zoom` | ⬜ planned |
| Add/Edit 3D | `tool.annot.edit3D` | ⬜ planned |
| Add/Edit Sound | `tool.annot.editSound` | ✅ available |
| Add/Edit Video | `tool.annot.editVideo` | ✅ available |
| Edit 3 D | `tool.edit3d` | ⬜ planned |
| ↳ Edit JavaScript | `tool.edit3d.editJS` | ⬜ planned |
| ↳ Poster | `tool.edit3d.poster` | ⬜ planned |
| Media | `media` | ✅ available |
| ↳ Media Configuration | `media.config` | ✅ available |
| ↳ Media Controls | `media.contol` | ✅ available |
| ↳ Media List | `media.list` | ✅ available |
| ↳ Mute | `media.mute` | ✅ available |
| ↳ Pause | `media.pause` | ✅ available |
| ↳ Play | `media.play` | ✅ available |
| ↳ Play Pause | `media.play_pause` | ✅ available |
| ↳ Position | `media.position` | ✅ available |
| ↳ Show Controls | `media.showControls` | ✅ available |
| ↳ Stop | `media.stop` | ✅ available |
| ↳ Volume | `media.volume` | ✅ available |
| Media Annotation | `mediaAnnot` | ✅ available |
| ↳ Disable Content | `mediaAnnot.disableContent` | ✅ available |
| ↳ Enable Content | `mediaAnnot.enableContent` | ✅ available |
| ↳ Properties | `mediaAnnot.properties` | ✅ available |
| ↳ Save Audio | `mediaAnnot.saveAudio` | ✅ available |
| ↳ Save Video | `mediaAnnot.saveVideo` | ✅ available |
| ↳ Start Full Screen | `mediaAnnot.startFullScreen` | ✅ available |
| ↳ Start View Floating | `mediaAnnot.startViewFloating` | ✅ available |
| ↳ Stop Full Screen | `mediaAnnot.stopFullScreen` | ✅ available |
| ↳ Stop View Floating | `mediaAnnot.stopViewFloating` | ✅ available |

**Sound**

| Command | Id | Status |
|---|---|---|
| Sound | `sound` | ✅ available |
| ↳ Mute | `sound.mute` | ✅ available |
| ↳ Pause | `sound.pause` | ✅ available |
| ↳ Play | `sound.play` | ✅ available |
| ↳ Save Audio | `sound.saveAudio` | ✅ available |
| ↳ Stop | `sound.stop` | ✅ available |

### Protect

**Digital Signatures**

| Command | Id | Status |
|---|---|---|
| Add Digital Signature… | `tool.digisig` | ✅ available |
| Digital IDs… | `signs.digitalIDs` | ✅ available |
| Digital Signature | `digisig` | ✅ available |
| ↳ Certify | `digisig.certify` | ✅ available |
| ↳ Clear… | `digisig.clear` | ✅ available |
| ↳ Sign | `digisig.sign` | ✅ available |
| ↳ Summary | `digisig.summary` | ✅ available |
| ↳ Validate | `digisig.validate` | ✅ available |
| ↳ View | `digisig.view` | ✅ available |
| Digital Signatures | `digiSigList` | ⬜ planned |
| ↳ Clone… | `digiSigList.clone` | ⬜ planned |
| ↳ Delete | `digiSigList.delete` | ⬜ planned |
| ↳ Edit… | `digiSigList.edit` | ⬜ planned |
| ↳ Reset | `digiSigList.reset` | ⬜ planned |
| Digital Signatures | `document.signatures` | 🟡 partial |
| ↳ Certify (Visible)… | `document.signatures.certify` | ✅ available |
| ↳ Clear All Signatures | `document.signatures.clear` | ✅ available |
| ↳ Sign Document… | `document.signatures.sign` | ✅ available |
| ↳ Signatures and Initials | `document.signatures.placesign` | ✅ available |
| ↳ Timestamp… | `document.signatures.timestamp` | ⬜ planned |
| ↳ Validate All Signatures | `document.signatures.validateAll` | ✅ available |
| Multi-Place Signature… | `document.signature.multiPlace` | ✅ available |

**Signatures and Initials**

| Command | Id | Status |
|---|---|---|
| Add Signature Placeholder | `tool.signMarkup` | ✅ available |
| Free Sign | `tool.freeSign` | ✅ available |
| Sign Markup | `signMarkup` | ✅ available |
| ↳ Popup | `signMarkup.popup` | ✅ available |
| Signs | `signs` | ✅ available |
| ↳ Add | `signs.add` | ✅ available |
| ↳ Apply All | `signs.applyAll` | ✅ available |
| ↳ Apply Signature | `signs.applySignature` | ✅ available |
| ↳ Cancel All | `signs.cancelAll` | ✅ available |
| ↳ Digital IDs… | `signs.digitalIDs` | ✅ available |
| ↳ Edit… | `signs.edit` | ✅ available |
| ↳ Export… | `signs.export` | ✅ available |
| ↳ Import… | `signs.import` | ✅ available |
| ↳ Manage… | `signs.manage` | ✅ available |
| ↳ Placeholders | `signs.markup` | ✅ available |
| ↳ Remove | `signs.remove` | ✅ available |

**Redact**

| Command | Id | Status |
|---|---|---|
| Apply All Redactions | `document.applyRedactions` | ✅ available |
| Apply Selected Redactions | `annots.applyRedactions` | ✅ available |
| Overlay | `overlay` | ✅ available |
| ↳ Fill Color | `overlay.fcolor` | ✅ available |
| ↳ Font Name | `overlay.fontName` | ✅ available |
| ↳ Font Size | `overlay.fontSize` | ✅ available |
| ↳ Repeat Text | `overlay.repeatText` | ✅ available |
| ↳ Text Color | `overlay.textColor` | ✅ available |
| Redaction | `addAnnotsByFindResult.redaction` | ✅ available |
| Redaction | `addAnnotsBySearchResult.redaction` | ✅ available |
| Redaction | `document.redaction` | ✅ available |
| Redaction | `redaction` | ✅ available |
| ↳ Add Code | `redaction.addTextCode` | ✅ available |
| ↳ Add Code Set | `redaction.addTextCodeSet` | ✅ available |
| ↳ Overlay | `redaction.overlay` | ✅ available |
| ↳ Overlay Type | `redaction.overlayType` | ✅ available |

**Document Security**

| Command | Id | Status |
|---|---|---|
| Crypt Alter Bar | `cryptAlterBar` | ⬜ planned |
| ↳ Allow | `cryptAlterBar.allow` | ⬜ planned |
| ↳ Options | `cryptAlterBar.opts` | ⬜ planned |
| Public Key Security | `pubsec` | ✅ available |
| ↳ Cert | `pubsec.cert` | ✅ available |
| Save Unrestricted Copy… | `saveUnrestricted` | ✅ available |
| Security Policies | `secPoliciesList` | ✅ available |
| ↳ Add | `secPoliciesList.add` | ✅ available |
| ↳ Clone… | `secPoliciesList.clone` | ✅ available |
| ↳ Delete | `secPoliciesList.delete` | ✅ available |
| ↳ Edit… | `secPoliciesList.edit` | ✅ available |
| ↳ Export… | `secPoliciesList.export` | ✅ available |
| ↳ Import… | `secPoliciesList.import` | ✅ available |
| ↳ Rename… | `secPoliciesList.rename` | ✅ available |
| Security Policies | `securityPolicies` | ✅ available |
| Standard Security Policy… | `standardSecPolicy` | ✅ available |

**E-Signature**

| Command | Id | Status |
|---|---|---|
| E-Signature Service | `document.esign` | ⬜ planned |
| ↳ Send for E-Signature | `document.esign.send` | ⬜ planned |
| ↳ Sign with the E-Signature Service | `document.esign.sign` | ⬜ planned |
| E-Signature Service | `esign` | ⬜ planned |
| ↳ Change Account… | `esign.changeAccount` | ⬜ planned |
| ↳ Create Envelope… | `esign.createEnvelope` | ⬜ planned |
| ↳ Log In to the E-Signature Service… | `esign.login` | ⬜ planned |
| ↳ Log Out of the E-Signature Service | `esign.logout` | ⬜ planned |

### Form

**Form Fields**

| Command | Id | Status |
|---|---|---|
| Button | `btn` | ✅ available |
| ↳ Behaviour | `btn.behaviour` | ✅ available |
| ↳ Down Icon | `btn.downIcon` | ✅ available |
| ↳ Roll Over Icon | `btn.rollOverIcon` | ✅ available |
| ↳ Up Icon | `btn.upIcon` | ✅ available |
| Combobox | `combobox` | ✅ available |
| ↳ Items | `combobox.items` | ✅ available |
| Edit Document Forms | `tool.edit.form` | ⬜ planned |
| Edit Form | `tool.editFields` | ✅ available |
| ↳ Add New Field | `tool.editFields.addNewField` | ✅ available |
| ↳ Create Multi | `tool.editFields.createMulti` | ✅ available |
| ↳ Field Type | `tool.editFields.mode` | 🟡 partial |
| ↳ Keep Mode | `tool.editFields.keepMode` | ✅ available |
| ↳ Rename Field | `tool.editFields.renameField` | ✅ available |
| ↳ Set as Required | `tool.editFields.setAsRequired` | ✅ available |
| ↳ Show Calculation Ord Index | `tool.editFields.showCalcOrdIdx` | ✅ available |
| ↳ Show Tab Numbers | `tool.editFields.showTabNumbers` | ✅ available |
| Field Editor | `editFields` | ✅ available |
| ↳ Items | `editFields.items` | ✅ available |
| Listbox | `listbox` | ✅ available |
| ↳ Items | `listbox.items` | ✅ available |
| Select | `fieldsView.select` | ✅ available |
| ↳ Barcode | `fieldsView.select.barcode` | ✅ available |
| ↳ Button | `fieldsView.select.button` | ✅ available |
| ↳ Check | `fieldsView.select.check` | ✅ available |
| ↳ Combo Box | `fieldsView.select.combo` | ✅ available |
| ↳ List | `fieldsView.select.list` | ✅ available |
| ↳ Radio | `fieldsView.select.radio` | ✅ available |
| ↳ Signature | `fieldsView.select.signature` | ✅ available |
| ↳ Text Fields | `fieldsView.select.text` | ✅ available |
| Tab Order | `fieldsView.tabOrder` | ✅ available |
| ↳ Column | `fieldsView.tabOrder.column` | ✅ available |
| ↳ Manually | `fieldsView.tabOrder.manually` | ✅ available |
| ↳ Row | `fieldsView.tabOrder.row` | ✅ available |
| ↳ Structure | `fieldsView.tabOrder.structure` | ✅ available |
| ↳ Unspecified | `fieldsView.tabOrder.unspecified` | ✅ available |
| Widget | `widget` | ✅ available |
| ↳ Font Size | `widget.fontSize` | ✅ available |

**Form Data**

| Command | Id | Status |
|---|---|---|
| Forms | `forms` | 🟡 partial |
| ↳ Email Form Data… | `forms.emailData` | ✅ available |
| ↳ Export Data… | `forms.exportData` | ✅ available |
| ↳ Highlight Form Fields | `forms.highlight` | ✅ available |
| ↳ Import Data… | `forms.importData` | ✅ available |
| ↳ Manage Form Data | `forms.manageData` | ⬜ planned |
| ↳ Populate Forms… | `forms.populate` | ✅ available |
| ↳ Reset Form | `forms.reset` | ✅ available |
| ↳ Set Field Calculation Order… | `forms.calcOrder` | ✅ available |

**JavaScript**

| Command | Id | Status |
|---|---|---|
| Action | `action` | ⬜ planned |
| ↳ Go to | `action.goto` | ⬜ planned |
| Actions | `actions` | ⬜ planned |
| ↳ Add | `actions.add` | ⬜ planned |
| ↳ Add from Bookmark | `actions.addFromBookmark` | ⬜ planned |
| ↳ Down | `actions.down` | ⬜ planned |
| ↳ Edit… | `actions.edit` | ⬜ planned |
| ↳ Remove | `actions.remove` | ⬜ planned |
| ↳ Up | `actions.up` | ⬜ planned |
| Document Actions | `docActions` | ⬜ planned |
| ↳ Clear… | `docActions.clear` | ⬜ planned |
| Document Actions | `document.actions` | ✅ available |
| Document JavaScript | `document.javaScript` | ✅ available |
| Java Script | `javaScript` | ✅ available |
| ↳ Edit All | `javaScript.editAll` | ✅ available |
| JavaScript Console | `jsConsole` | ✅ available |
| ↳ Clear… | `jsConsole.clear` | ✅ available |
| ↳ Open Folder | `jsConsole.openFolder` | ✅ available |
| ↳ Preferences… | `jsConsole.prefs` | ✅ available |
| ↳ Run | `jsConsole.run` | ✅ available |
| ↳ Run Select | `jsConsole.runSelect` | ✅ available |
| ↳ Show | `jsConsole.show` | ✅ available |
| JavaScript Edit Snippets | `jsEditSnippets` | ✅ available |
| JavaScript List | `javaScriptList` | ✅ available |
| ↳ Add | `javaScriptList.add` | ✅ available |
| ↳ Delete | `javaScriptList.delete` | ✅ available |
| ↳ Edit… | `javaScriptList.edit` | ✅ available |
| JavaScript Snippet | `jssnippet` | ✅ available |
| ↳ Add | `jssnippet.add` | ✅ available |
| ↳ Clone… | `jssnippet.clone` | ✅ available |
| ↳ Delete | `jssnippet.delete` | ✅ available |
| Layer Actions | `dao` | ⬜ planned |
| ↳ Change State | `dao.changeState` | ⬜ planned |
| ↳ Simplify Actions | `dao.simplifyActions` | ⬜ planned |
| ↳ Used Data | `dao.usedData` | ⬜ planned |

### Organize

**Pages**

| Command | Id | Status |
|---|---|---|
| Delete Empty Pages… | `document.deleteEmptyPages` | ✅ available |
| Delete Pages… | `document.deletePages` | ✅ available |
| Duplicate Pages… | `document.duplicatePages` | ✅ available |
| Extract Pages… | `document.extractPages` | ✅ available |
| Find Duplicate Pages | `document.findDuplicates` | ✅ available |
| Insert Blank Pages… | `document.insertEmptyPages` | ✅ available |
| Insert Images… | `document.insertImages` | ✅ available |
| Insert Pages from Clipboard… | `document.insertPagesFromClipboard` | ✅ available |
| Insert Pages… | `document.insertPages` | ✅ available |
| Insert Rich Text… | `document.insertRTF` | ✅ available |
| Insert Scanned Pages… | `document.insertScannedPages` | ⬜ planned |
| Insert Text… | `document.insertText` | ✅ available |
| Merge Pages… | `document.mergePages` | ✅ available |
| More Page Tools | `document.pagesMore` | ⬜ planned |
| Move Pages… | `document.movePages` | ✅ available |
| New Document from Selection… | `document.createNewDocFromSel` | ✅ available |
| Replace Pages… | `document.replacePages` | ✅ available |
| Reverse Page Order | `document.reversePageOrder` | ✅ available |
| Select Pages | `selectPages` | ✅ available |
| Split | `document.splitGroup` | ✅ available |
| Split Document… | `document.split` | ✅ available |
| Split Pages | `splitPages` | ✅ available |
| ↳ Add Horizontal | `splitPages.addHorizontal` | ✅ available |
| ↳ Add Vertical | `splitPages.addVertical` | ✅ available |
| ↳ Clear… | `splitPages.clear` | ✅ available |
| ↳ Color Picker | `splitPages.colorPicker` | ✅ available |
| ↳ Edit Line | `splitPages.editLine` | ✅ available |
| ↳ Every | `splitPages.every` | ✅ available |
| ↳ Remove Line | `splitPages.removeLine` | ✅ available |
| ↳ Show Rulers | `splitPages.showRulers` | ✅ available |
| ↳ Units | `splitPages.units` | ✅ available |
| ↳ Zoom Combo | `splitPages.zoomCombo` | ✅ available |
| ↳ Zoom Out | `splitPages.zoomOut` | ✅ available |
| ↳ Zoom in | `splitPages.zoomIn` | ✅ available |
| Split Pages by Guides… | `document.splitPagesByGuides` | ✅ available |
| Split Pages… | `document.splitPages` | ✅ available |
| Swap Pages… | `document.swapPages` | ✅ available |

**Transform Pages**

| Command | Id | Status |
|---|---|---|
| Crop | `crop` | ⬜ planned |
| ↳ Apply | `crop.apply` | ⬜ planned |
| Crop Page | `cropPage` | ⬜ planned |
| ↳ Apply | `cropPage.apply` | ⬜ planned |
| ↳ Hide All Boxes | `cropPage.hideAllBoxes` | ⬜ planned |
| ↳ Hide Origin Anchor | `cropPage.hideOriginAnchor` | ⬜ planned |
| ↳ Show All Boxes | `cropPage.showAllBoxes` | ⬜ planned |
| ↳ Show Origin Anchor | `cropPage.showOriginAnchor` | ⬜ planned |
| Crop Page Tool | `tool.cropPage` | ✅ available |
| Crop Pages… | `document.cropPages` | ✅ available |
| Crop to White Margins | `document.cropToWhiteMargins` | ✅ available |
| ↳ Bottom | `document.cropToWhiteMargins.bottom` | ✅ available |
| ↳ Left | `document.cropToWhiteMargins.left` | ✅ available |
| ↳ Right | `document.cropToWhiteMargins.right` | ✅ available |
| ↳ Top | `document.cropToWhiteMargins.top` | ✅ available |
| Normalize Pages… | `document.normalizePages` | ✅ available |
| Overlay Pages… | `document.overlayPages` | ✅ available |
| Rasterize Pages… | `document.rasterizePages` | ✅ available |
| Remove Cropped Content | `document.removeCroppedContent` | 🟡 partial |
| Resize Pages… | `document.resizePages` | ✅ available |
| Rotate Pages… | `document.rotatePages` | ✅ available |
| ↳ Rotate 180° | `document.rotatePages.180` | ✅ available |
| ↳ Rotate 90° Clockwise | `document.rotatePages.cw` | ✅ available |
| ↳ Rotate 90° Counterclockwise | `document.rotatePages.ccw` | ✅ available |
| Set Margins… | `document.setMargins` | ✅ available |

**Page Marks**

| Command | Id | Status |
|---|---|---|
| Background | `document.background` | ✅ available |
| ↳ Add… | `document.background.add` | ✅ available |
| ↳ Remove… | `document.background.remove` | ✅ available |
| ↳ Update… | `document.background.update` | ✅ available |
| Bates Numbering | `document.batesNumbering` | ✅ available |
| ↳ Add to Multiple Documents… | `document.batesNumbering.addMulti` | ✅ available |
| ↳ Add… | `document.batesNumbering.add` | ✅ available |
| ↳ Manage… | `document.batesNumbering.manage` | ✅ available |
| ↳ Remove All… | `document.batesNumbering.removeAll` | ✅ available |
| Header and Footer | `document.headerAndFooter` | ✅ available |
| ↳ Add… | `document.headerAndFooter.add` | ✅ available |
| ↳ Manage… | `document.headerAndFooter.manage` | ✅ available |
| ↳ Remove All… | `document.headerAndFooter.removeAll` | ✅ available |
| Macro | `macro` | 🟡 partial |
| ↳ Application Version | `macro.appVersion` | ✅ available |
| ↳ Auto-Number | `macro.autonumber` | ✅ available |
| ↳ Bates Number | `macro.bates` | ✅ available |
| ↳ Bates Value | `macro.batesValue` | ✅ available |
| ↳ Bookmark Title | `macro.bookmarkTitle` | ✅ available |
| ↳ Computer | `macro.computer` | ✅ available |
| ↳ Date | `macro.date` | ✅ available |
| ↳ Day | `macro.day` | ✅ available |
| ↳ Docnumber | `macro.docnumber` | ✅ available |
| ↳ Document Info | `macro.docInfo` | ✅ available |
| ↳ Document Title | `macro.docTitle` | ✅ available |
| ↳ Env | `macro.env` | ✅ available |
| ↳ File Extension | `macro.fileExt` | ✅ available |
| ↳ File Name | `macro.fileName` | ✅ available |
| ↳ Folder Name | `macro.folderName` | ✅ available |
| ↳ Folder Path | `macro.folderPath` | ✅ available |
| ↳ Folder Title | `macro.folderTitle` | ✅ available |
| ↳ Hour | `macro.hour` | ✅ available |
| ↳ Imagenumber | `macro.imagenumber` | ✅ available |
| ↳ Launch Application | `macro.LaunchApp` | ✅ available |
| ↳ Macro Guide | `macro.guide` | ✅ available |
| ↳ Minute | `macro.minute` | ✅ available |
| ↳ Month | `macro.month` | ✅ available |
| ↳ Page Count | `macro.pages` | ✅ available |
| ↳ Page Label | `macro.pageLabel` | ✅ available |
| ↳ Page Number | `macro.page` | ✅ available |
| ↳ Page Numbering Style | `macro.pageNumberingStyle` | ✅ available |
| ↳ Region | `macro.region` | 🟡 partial |
| ↳ Row Data | `macro.rowData` | ⬜ planned |
| ↳ Row Data Index | `macro.rowDataIndex` | ⬜ planned |
| ↳ Row Data Name | `macro.rowDataName` | ⬜ planned |
| ↳ Row Index | `macro.rowIndex` | ⬜ planned |
| ↳ Seconds | `macro.seconds` | ✅ available |
| ↳ Source Pages | `macro.srcPages` | ✅ available |
| ↳ Time | `macro.time` | ✅ available |
| ↳ User | `macro.user` | ✅ available |
| ↳ Year | `macro.year` | ✅ available |
| Page Marks | `pagemarks` | 🟡 partial |
| ↳ Add | `pagemarks.add` | ✅ available |
| ↳ Edit… | `pagemarks.edit` | ✅ available |
| ↳ Remove | `pagemarks.remove` | ✅ available |
| ↳ Remove All | `pagemarks.removeAll` | ✅ available |
| ↳ Zoom Out | `pagemarks.zoomOut` | ⬜ planned |
| ↳ Zoom in | `pagemarks.zoomIn` | ⬜ planned |
| Page Numbering… | `document.pageNumbering` | ✅ available |
| Page Transitions | `document.pageTransitions` | ✅ available |
| Watermarks | `document.watermarks` | ✅ available |
| ↳ Add… | `document.watermarks.add` | ✅ available |
| ↳ Manage… | `document.watermarks.manage` | ✅ available |
| ↳ Remove All… | `document.watermarks.removeAll` | ✅ available |

### Convert

**Create**

| Command | Id | Status |
|---|---|---|
| Blank Pages… | `newDocBlank` | ✅ available |
| Combine Files into One PDF… | `newDocCombine` | ✅ available |
| Configure Scan Presets… | `configureScanPresets` | ⬜ planned |
| Custom Scan… | `newDocCustomScan` | ⬜ planned |
| From Clipboard… | `newDocClipboard` | ✅ available |
| From Email… | `newDocEmail` | ✅ available |
| From Files… | `newDocFromFiles` | ✅ available |
| From Images… | `newDocImage` | ✅ available |
| From Rich Text… | `newDocRTF` | ⬜ planned |
| From Scanner… | `newDocScanner` | ⬜ planned |
| From Selection… | `newDocSelection` | ⬜ planned |
| From Text… | `newDocText` | ✅ available |
| From Web Page… | `newDocHtml` | ⬜ planned |
| New Document | `newDoc` | ✅ available |
| PDF Portfolio… | `newPortfolio` | ✅ available |
| Source Files | `newDocumentList` | 🟡 partial |
| ↳ Add File… | `newDocumentList.addFile` | ✅ available |
| ↳ Add Folder… | `newDocumentList.addFolder` | ✅ available |
| ↳ Bottom | `newDocumentList.bottom` | ✅ available |
| ↳ Clear… | `newDocumentList.clear` | ✅ available |
| ↳ Down | `newDocumentList.down` | ✅ available |
| ↳ Edit… | `newDocumentList.edit` | ✅ available |
| ↳ Import… | `newDocumentList.import` | ✅ available |
| ↳ Range | `newDocumentList.range` | ✅ available |
| ↳ Remove | `newDocumentList.remove` | ✅ available |
| ↳ Top | `newDocumentList.top` | ✅ available |
| ↳ Up | `newDocumentList.up` | ✅ available |
| ↳ View Mode | `newDocumentList.viewMode` | ✅ available |
| ↳ Webcam | `newDocumentList.webcam` | ⬜ planned |

**Export**

| Command | Id | Status |
|---|---|---|
| Export | `document.export` | ✅ available |
| ↳ Export to Images… | `document.export.toImages` | ✅ available |
| Export | `export` | ✅ available |
| Export All Bookmarks… | `document.exportBookmarks` | ✅ available |
| Export Selected Bookmarks… | `document.exportSelBookmarks` | ✅ available |
| Export to CSV… | `linksView.exportToCSV` | ✅ available |
| Export… | `linksView.export` | ✅ available |
| Save Image As… | `saveImage` | ✅ available |

**Modify**

| Command | Id | Status |
|---|---|---|
| Audit Space Usage | `document.audit` | ✅ available |
| Detect Barcodes in Region… | `document.detectBarcodes` | ✅ available |
| Detect Barcodes… | `document.detectImageBarcode` | ✅ available |
| Enable Editing (Discard PDF/A) | `document.discardPDFA` | ✅ available |
| Font | `font` | ⬜ planned |
| ↳ Repair to Unicode C Map | `font.repairToUnicodeCMap` | ⬜ planned |
| Highlight from File | `document.highlightFile` | ✅ available |
| Import Bookmarks | `document.importBookmarks` | ✅ available |
| New Stamp from Active Document | `document.createStampFromActiveDoc` | ✅ available |
| New Stamp from Selection… | `document.createStampFromSel` | ✅ available |
| Options… | `document.options` | ⬜ planned |
| Replace Fonts… | `document.replaceFonts` | ⬜ planned |
| Show Problem Details… | `document.showProblems` | ⬜ planned |
| Substitute Fonts | `saf` | ⬜ planned |
| ↳ Change Font… | `saf.changeFont` | ⬜ planned |
| ↳ Reset All Fonts | `saf.resetAllFonts` | ⬜ planned |
| ↳ Reset Font | `saf.resetFont` | ⬜ planned |
| Word Count | `document.wordCount` | ✅ available |
| XMP | `xmp` | ✅ available |
| ↳ Export | `xmp.export` | ✅ available |
| ↳ Show | `xmp.show` | ✅ available |

**OCR**

| Command | Id | Status |
|---|---|---|
| OCR Images… | `document.ocrImages` | ✅ available |
| OCR Image… | `OCRImage` | ✅ available |
| OCR Pages… | `document.OCRPages` | ✅ available |
| OCR Selected Region… | `document.OCRSelRgn` | ✅ available |

**Enhance Pages**

| Command | Id | Status |
|---|---|---|
| Edit Image | `editImage` | ✅ available |
| ↳ Apply to All | `editImage.applyToAll` | ✅ available |
| ↳ Builtin | `editImage.builtin` | ✅ available |
| ↳ Default | `editImage.default` | ✅ available |
| ↳ Edit Type | `editImage.editType` | ✅ available |
| ↳ Edit with | `editImage.editWith` | ✅ available |
| Image Editor | `imageEditor` | ✅ available |
| ↳ Convert | `imageEditor.convert` | ✅ available |
| ↳ Crop | `imageEditor.crop` | ✅ available |
| ↳ Deskew | `imageEditor.deskew` | ✅ available |
| ↳ Filters | `imageEditor.filters` | ✅ available |
| ↳ Fit Visible | `imageEditor.fitVisible` | ✅ available |
| ↳ Flip Horizontal | `imageEditor.flipHorizontal` | ✅ available |
| ↳ Flip Vertical | `imageEditor.flipVertical` | ✅ available |
| ↳ Grayscale | `imageEditor.grayscale` | ✅ available |
| ↳ Rotate 90 Clockwise | `imageEditor.rotate90CW` | ✅ available |
| ↳ Rotate 90 Counterclockwise | `imageEditor.rotate90CCW` | ✅ available |
| ↳ Zoom | `imageEditor.zoom` | ✅ available |
| ↳ Zoom Out | `imageEditor.zoomOut` | ✅ available |
| ↳ Zoom in | `imageEditor.zoomIn` | ✅ available |
| Recompress Images | `recompressImages` | ⬜ planned |
| ↳ Edit… | `recompressImages.edit` | ⬜ planned |
| Recompress Images… | `document.recompressImages` | ✅ available |
| Recompress Image… | `document.recompressImage` | 🟡 partial |
| Replace Image | `replaceImage` | ✅ available |
| Replace Image Clipboard | `replaceImageClipboard` | ✅ available |

### Share

**Send To**

| Command | Id | Status |
|---|---|---|
| Add Place | `addPlace` | ✅ available |
| Browse Places… | `openFromPlaces` | ✅ available |
| Browse Places… | `saveToPlaces` | ✅ available |
| Email All Open Documents as ZIP… | `sendByEmailOpenAllZip` | ✅ available |
| Email All Open Documents… | `sendByEmailOpenAll` | ✅ available |
| Email Document… | `sendByEmail` | ✅ available |
| Email Methods | `emailMethods` | ⬜ planned |
| ↳ Manage… | `emailMethods.manage` | ⬜ planned |
| Email as ZIP… | `sendByEmailZip` | ✅ available |
| Explorer | `explorer` | ✅ available |
| ↳ Delete | `explorer.delete` | ✅ available |
| ↳ Folder | `explorer.folder` | ✅ available |
| ↳ Places | `explorer.places` | ✅ available |
| ↳ Refresh | `explorer.refresh` | ✅ available |
| ↳ Thumbnail | `explorer.thumb` | ✅ available |
| Folder Filters | `folderExts` | ✅ available |
| ↳ Add Custom… | `folderExts.addCustom` | ✅ available |
| ↳ Edit Custom… | `folderExts.editCustom` | ✅ available |
| ↳ File List | `folderExts.fileList` | ✅ available |
| ↳ Remove Custom… | `folderExts.removeCustom` | ✅ available |
| Places List Open | `placesListOpen` | ✅ available |
| Places List Save | `placesListSave` | ✅ available |
| Share | `shareGroup` | ⬜ planned |
| WebDAV | `wdav` | ⬜ planned |
| ↳ Columns | `wdav.columns` | ⬜ planned |
| ↳ Copy | `wdav.copy` | ⬜ planned |
| ↳ Copy as Path | `wdav.copyAsPath` | ⬜ planned |
| ↳ Cut | `wdav.cut` | ⬜ planned |
| ↳ Delete | `wdav.delete` | ⬜ planned |
| ↳ Paste | `wdav.paste` | ⬜ planned |
| ↳ Rename… | `wdav.rename` | ⬜ planned |
| ↳ Settings… | `wdav.settings` | ⬜ planned |
| ↳ View | `wdav.view` | ⬜ planned |

### Review

**Proofing**

| Command | Id | Status |
|---|---|---|
| Spell Check and Replace | `document.spellCheckAndRepl` | ✅ available |
| ↳ Check Spelling | `document.spellCheckAndRepl.forward` | ✅ available |
| ↳ Ignore All Spelling Errors | `document.spellCheckAndRepl.ignoreAll` | ✅ available |
| ↳ Ignore Spelling Error | `document.spellCheckAndRepl.ignore` | ✅ available |
| ↳ Previous Spelling Error | `document.spellCheckAndRepl.back` | ✅ available |

**Comments**

| Command | Id | Status |
|---|---|---|
| Compare Documents | `cmpDocs` | 🟡 partial |
| ↳ Filter | `cmpDocs.filter` | ✅ available |
| ↳ Go to | `cmpDocs.goto` | ✅ available |
| ↳ Show | `cmpDocs.show` | 🟡 partial |
| Compare Documents… | `compareDocs` | ✅ available |
| Comparison | `comparisonView` | ✅ available |
| ↳ First | `comparisonView.first` | ✅ available |
| ↳ Last | `comparisonView.last` | ✅ available |
| ↳ Next | `comparisonView.next` | ✅ available |
| ↳ Previous | `comparisonView.prev` | ✅ available |
| ↳ Show Differs | `comparisonView.showDiffers` | ✅ available |

### Accessibility

**Auto-Tag**

| Command | Id | Status |
|---|---|---|
| Auto-Tag | `acc.autotag` | ✅ available |
| ↳ Auto-Tag Document | `acc.autotag.document` | ✅ available |
| ↳ Auto-Tag Form Fields | `acc.autotag.form` | ✅ available |

**Check**

| Command | Id | Status |
|---|---|---|
| Accessibility Checker | `accCheckerView` | ✅ available |
| ↳ Options… | `accCheckerView.options` | ✅ available |
| Accessibility Report | `accReportView` | ✅ available |
| ↳ Options… | `accReportView.options` | ✅ available |
| Check | `acc.check` | ✅ available |
| ↳ Accessibility Report | `acc.check.report` | ✅ available |
| ↳ Check Accessibility | `acc.check.document` | ✅ available |
| Checker | `chekerView` | ✅ available |
| ↳ Explain | `chekerView.explain` | ✅ available |
| ↳ Fail | `chekerView.fail` | ✅ available |
| ↳ Fix | `chekerView.fix` | ✅ available |
| ↳ Options… | `chekerView.options` | ✅ available |
| ↳ Pass | `chekerView.pass` | ✅ available |
| ↳ Re Check | `chekerView.reCheck` | ✅ available |
| ↳ Show | `chekerView.show` | ✅ available |
| ↳ Show Report | `chekerView.showReport` | ✅ available |
| ↳ Skip | `chekerView.skip` | ✅ available |

**Edit**

| Command | Id | Status |
|---|---|---|
| Accessibility | `accessibility` | ✅ available |
| Alternate Text | `acc.alt` | ✅ available |
| Order Pane | `orderView` | ✅ available |
| ↳ Options… | `orderView.options` | ✅ available |
| Order Pane | `orderview` | ✅ available |
| ↳ Groupsimilar | `orderview.groupsimilar` | ✅ available |
| ↳ Show | `orderview.show` | ✅ available |
| ↳ Showmode | `orderview.showmode` | ✅ available |
| ↳ Tag | `orderview.tag` | ✅ available |
| Page Structure | `pageStructure` | ✅ available |
| ↳ Clear… | `pageStructure.clear` | ✅ available |
| ↳ Delete Item | `pageStructure.deleteItem` | ✅ available |
| Reading Order | `tool.order` | ⬜ planned |
| Tags | `tags` | ✅ available |
| Tags Pane | `tagsView` | ✅ available |
| ↳ Create Root | `tagsView.createRoot` | ✅ available |
| ↳ Options… | `tagsView.options` | ✅ available |
| Tags Pane | `tagsview` | ✅ available |
| ↳ Apply Role Map | `tagsview.applyRoleMap` | ✅ available |
| ↳ Change to Artifact | `tagsview.changeToArtifact` | ✅ available |
| ↳ Copy Contents | `tagsview.copyContents` | ✅ available |
| ↳ Create Tag from Selection | `tagsview.createTagFromSelection` | ✅ available |
| ↳ Delete Empty | `tagsview.deleteEmpty` | ✅ available |
| ↳ Edit Class Map | `tagsview.editClassMap` | ✅ available |
| ↳ Edit Role Map | `tagsview.editRoleMap` | ✅ available |
| ↳ Find Tag from Selection | `tagsview.findTagFromSelection` | ✅ available |
| ↳ Highlight Content | `tagsview.highlightContent` | ✅ available |
| ↳ Is Tagged | `tagsview.isTagged` | ✅ available |
| ↳ New Child | `tagsview.newChild` | ✅ available |
| ↳ New Sibling | `tagsview.newSibling` | ✅ available |
| ↳ New… | `tagsview.new` | ✅ available |
| ↳ Select Tag Content | `tagsview.selectTagContent` | ✅ available |
| ↳ Show in Content | `tagsview.showInContent` | ✅ available |

### Help

**UI Options**

| Command | Id | Status |
|---|---|---|
| Customize Toolbars | `customizeToolbars` | ✅ available |
| ↳ Delete | `customizeToolbars.delete` | ✅ available |
| ↳ Down | `customizeToolbars.down` | ✅ available |
| ↳ Make Default | `customizeToolbars.makeDefault` | ⬜ planned |
| ↳ New Tab | `customizeToolbars.newTab` | ⬜ planned |
| ↳ New Toolbar | `customizeToolbars.newToolbar` | ⬜ planned |
| ↳ New… | `customizeToolbars.new` | ⬜ planned |
| ↳ Rename… | `customizeToolbars.rename` | ⬜ planned |
| ↳ Reset | `customizeToolbars.reset` | ✅ available |
| ↳ Up | `customizeToolbars.up` | ✅ available |
| Item | `item` | 🟡 partial |
| ↳ Delete | `item.delete` | ✅ available |
| ↳ Hard Break Separator | `item.hardBreakSep` | ⬜ planned |
| ↳ Insert Separator After | `item.insertSepAfter` | ✅ available |
| ↳ Insert Separator Before | `item.insertSepBefore` | ✅ available |
| ↳ Invisible Separator | `item.invisibleSep` | ⬜ planned |
| ↳ Non Break Separator | `item.nonBreakSep` | ⬜ planned |
| ↳ Properties… | `item.props` | ✅ available |
| ↳ Show Icon | `item.showIcon` | ✅ available |
| ↳ Show Text | `item.showText` | ✅ available |
| Preferences… | `prefs` | ✅ available |
| ↳ Measurement Options… | `prefs.measurement` | ⬜ planned |
| Rename Toolbar | `renameToolbar` | ⬜ planned |
| Reset Menu | `resetMenu` | ⬜ planned |
| Reset Toolbar | `resetToolbar` | ✅ available |
| Settings | `settings` | ✅ available |
| ↳ Export Settings… | `settings.export` | ✅ available |
| ↳ Import Settings… | `settings.import` | ✅ available |
| ↳ Manage Settings… | `settings.manage` | ✅ available |
| ↳ Reset Settings… | `settings.reset` | ✅ available |
| Show Quick Access Toolbar Above Ribbon | `QABarAbove` | ✅ available |
| Show Quick Access Toolbar Below Ribbon | `QABarBelow` | ✅ available |

**Help**

| Command | Id | Status |
|---|---|---|
| Help | `help` | 🟡 partial |
| ↳ About Folitecta… | `help.about` | ✅ available |
| ↳ Check for Updates | `help.checkForUpdates` | ✅ available |
| ↳ Command-Line Help | `help.cmdline` | ✅ available |
| ↳ File Associations… | `help.fileAssoc` | ⬜ planned |
| ↳ Project Website | `help.homepage` | ✅ available |
| ↳ Report a Problem… | `help.report` | ✅ available |
| ↳ Support Forum | `help.support` | ✅ available |
| ↳ User Guide | `help.contents` | ✅ available |

**Product**

| Command | Id | Status |
|---|---|---|
| About Folitecta… | `help.about` | ✅ available |
| Check for Updates | `help.checkForUpdates` | ✅ available |
| Project Website | `help.homepage` | ✅ available |

## Classic menus

### File

| Command | Id | Status |
|---|---|---|
| Open… | `open` | ✅ available |
| Open From | `openFrom` | ✅ available |
| Recent Files | `recentFiles` | ✅ available |
| Sessions | `sessions` | 🟡 partial |
| New Document | `newDoc` | ✅ available |
| PDF Portfolio… | `newPortfolio` | ✅ available |
| Save | `save` | ✅ available |
| Save As… | `saveAs` | ✅ available |
| Save a Copy… | `saveAsCopy` | ✅ available |
| Save All | `saveAll` | ✅ available |
| Revert to Saved | `revert` | ✅ available |
| Close | `close` | ✅ available |
| Close All | `closeAll` | ✅ available |
| Export | `export` | ✅ available |
| Export | `document.export` | ✅ available |
| Email Document… | `sendByEmail` | ✅ available |
| Share | `shareGroup` | ⬜ planned |
| Print… | `print` | ✅ available |
| Print All… | `printAll` | ✅ available |
| Document Properties… | `docProps` | ✅ available |
| Security Properties… | `document.security` | ✅ available |
| Exit | `exit` | ✅ available |
| Add Place | `addPlace` | ✅ available |
| Bates Numbering Files | `batesNumberingFiles` | ⬜ planned |
| Blank Pages… | `newDocBlank` | ✅ available |
| Browse Places… | `openFromPlaces` | ✅ available |
| Browse Places… | `saveToPlaces` | ✅ available |
| Browse for Open | `browseForOpen` | ✅ available |
| Browse for Save | `browseForSave` | ✅ available |
| Combine Files into One PDF… | `newDocCombine` | ✅ available |
| Copy File | `copySrcFile` | 🟡 partial |
| Copy Full Path | `copySrcFileName` | ✅ available |
| Custom Scan… | `newDocCustomScan` | ⬜ planned |
| E-Signature Service | `document.esign` | ⬜ planned |
| E-Signature Service | `esign` | ⬜ planned |
| Email All Open Documents as ZIP… | `sendByEmailOpenAllZip` | ✅ available |
| Email All Open Documents… | `sendByEmailOpenAll` | ✅ available |
| Email Methods | `emailMethods` | ⬜ planned |
| Email as ZIP… | `sendByEmailZip` | ✅ available |
| Explorer | `explorer` | ✅ available |
| Folder Filters | `folderExts` | ✅ available |
| From Clipboard… | `newDocClipboard` | ✅ available |
| From Email… | `newDocEmail` | ✅ available |
| From Files… | `newDocFromFiles` | ✅ available |
| From Images… | `newDocImage` | ✅ available |
| From Rich Text… | `newDocRTF` | ⬜ planned |
| From Scanner… | `newDocScanner` | ⬜ planned |
| From Selection… | `newDocSelection` | ⬜ planned |
| From Text… | `newDocText` | ✅ available |
| From Web Page… | `newDocHtml` | ⬜ planned |
| History | `historyList` | ✅ available |
| History and Recents Options… | `manageHistory` | ✅ available |
| History… | `showHistory` | ⬜ planned |
| Manage Sessions… | `manageSessions` | ✅ available |
| Move File… | `moveDoc` | ✅ available |
| Move to Recycle Bin | `moveToTrash` | ✅ available |
| Open | `openGroup` | ✅ available |
| Open Containing Folder | `openSrcFolder` | ✅ available |
| Open Session from File… | `openSessionFromFile` | ✅ available |
| Open from URL… | `openFromURL` | ⬜ planned |
| Places List Open | `placesListOpen` | ✅ available |
| Places List Save | `placesListSave` | ✅ available |
| Recent | `recentsList` | ✅ available |
| Rename… | `renameDoc` | ✅ available |
| Reopen Closed Document | `reopenJustClosedDoc` | ✅ available |
| Save As | `saveAsGroup` | ✅ available |
| Save Copy As… | `saveCopyAs` | ✅ available |
| Save Current Session | `saveCurrentSession` | ✅ available |
| Save Current Session to File… | `saveCurrentSessionToFile` | ✅ available |
| Save Image As… | `saveImage` | ✅ available |
| Save Settings and History | `saveSettingsAndHistory` | ✅ available |
| Save To | `saveTo` | ✅ available |
| Save Unrestricted Copy… | `saveUnrestricted` | ✅ available |
| Sessions | `sessionsList` | ✅ available |
| Settings | `settings` | ✅ available |
| Source Files | `newDocumentList` | 🟡 partial |
| This Computer… | `openFromComp` | ✅ available |
| This Computer… | `saveToComp` | ✅ available |
| WebDAV | `wdav` | ⬜ planned |
| XMP | `xmp` | ✅ available |

### Edit

| Command | Id | Status |
|---|---|---|
| Undo | `undo` | ✅ available |
| Redo | `redo` | ✅ available |
| Cut | `cut` | ✅ available |
| Copy | `copy` | ✅ available |
| Paste | `paste` | ✅ available |
| Paste Options | `pasteSpecial` | ✅ available |
| Delete | `delete` | ✅ available |
| Select All | `selectAll` | ✅ available |
| Invert Selection | `invertSelection` | ✅ available |
| Deselect | `edit.deselect` | ✅ available |
| Find… | `find` | ✅ available |
| Find and Replace… | `findAndRepl` | ✅ available |
| Search… | `search` | ✅ available |
| Insert Text | `pageText.insert` | ✅ available |
| Replace Text | `pageText.replace` | ✅ available |
| Delete Text | `pageText.delete` | ✅ available |
| Arrange | `arrange` | ✅ available |
| Object | `object` | ⬜ planned |
| Preferences… | `prefs` | ✅ available |
| Add Object Group | `addObjectGroup` | ⬜ planned |
| Align | `align` | ✅ available |
| Align To | `alignto` | ✅ available |
| Choose Layer | `chooseLayer` | ⬜ planned |
| Clipboard | `clipboardGroup` | ⬜ planned |
| Crop | `crop` | ⬜ planned |
| Crop Page | `cropPage` | ⬜ planned |
| Customize Toolbars | `customizeToolbars` | ✅ available |
| Default Layer | `defaultLayer` | ⬜ planned |
| Distribute Horizontally | `distribHorz` | 🟡 partial |
| Distribute Vertically | `distribVert` | 🟡 partial |
| Distribute to | `distributeTo` | 🟡 partial |
| Document Size | `dsd` | ✅ available |
| Edit Image | `editImage` | ✅ available |
| Edit Link | `editLink` | ⬜ planned |
| Edit Object | `editObject` | ⬜ planned |
| Eraser Diameter | `eraserDiameter` | ⬜ planned |
| Fill Rule | `fillRule` | ⬜ planned |
| Filter Content Items | `filterContentItems` | ⬜ planned |
| Filter Image Items | `filterImageItems` | ⬜ planned |
| Filter Shapes | `filterShapes` | ⬜ planned |
| Filter Text Items | `filterTextItems` | ⬜ planned |
| Find | `findGroup` | ✅ available |
| Font | `font` | ⬜ planned |
| Font | `fontName` | 🟡 partial |
| Font Size | `fontSize` | ✅ available |
| Group | `group` | ✅ available |
| Guides Select All | `guidesSelectAll` | ⬜ planned |
| Guides Select on Page | `guidesSelectOnPage` | ⬜ planned |
| Image Editor | `imageEditor` | ✅ available |
| Ink Smooth Level | `inkSmoothLevel` | ⬜ planned |
| Item | `item` | 🟡 partial |
| Line Width Profile | `lineWidthProfile` | ⬜ planned |
| List | `list` | ⬜ planned |
| Make Same Height | `makeSameHeight` | ✅ available |
| Make Same Size | `makeSameSize` | ✅ available |
| Make Same Width | `makeSameWidth` | ✅ available |
| Manage Search Providers | `manageSearchProviders` | ⬜ planned |
| Move to New Layer | `moveToNewLayer` | ⬜ planned |
| Order | `order` | 🟡 partial |
| Paste Annotations | `pasteAnnots` | ✅ available |
| Paste Bookmarks | `pasteBookmarks` | ✅ available |
| Paste Contents | `pasteContents` | ⬜ planned |
| Paste Fields | `pasteFields` | ⬜ planned |
| Paste Files | `pasteFiles` | ⬜ planned |
| Paste Guides | `pasteGuides` | ⬜ planned |
| Paste Image | `pasteImage` | ✅ available |
| Paste Metafile | `pasteMetafile` | ⬜ planned |
| Paste Pages | `pastePages` | ⬜ planned |
| Paste Pages Special | `pastePagesSpecial` | ⬜ planned |
| Paste Plain Text | `pastePlainText` | ✅ available |
| Paste Rich Text | `pasteRichText` | ⬜ planned |
| Paste as Stamp | `pasteAsStamp` | ⬜ planned |
| Path Editing | `pathedit` | ⬜ planned |
| Position | `position` | ⬜ planned |
| Presets | `presetList` | ⬜ planned |
| Presets | `presets` | ⬜ planned |
| Properties… | `props` | ✅ available |
| Remove Clip | `removeClip` | ⬜ planned |
| Rename Toolbar | `renameToolbar` | ⬜ planned |
| Rename… | `rename` | ✅ available |
| Replace All Image Instances | `replaceAllImageInstances` | ⬜ planned |
| Replace Image | `replaceImage` | ✅ available |
| Replace Image Clipboard | `replaceImageClipboard` | ✅ available |
| Reset Menu | `resetMenu` | ⬜ planned |
| Reset Toolbar | `resetToolbar` | ✅ available |
| Search Providers | `searchProviders` | ⬜ planned |
| Select All Content Items | `selectAllContentItems` | ⬜ planned |
| Select Content Items | `selectContentItems` | ⬜ planned |
| Select Image Items | `selectImageItems` | ⬜ planned |
| Select Pages | `selectPages` | ✅ available |
| Select Shapes | `selectShapes` | ⬜ planned |
| Select Text Items | `selectTextItems` | ⬜ planned |
| Select X Form Items | `selectXFormItems` | ⬜ planned |
| Selection | `selectionGroup` | ⬜ planned |
| Show Quick Access Toolbar Above Ribbon | `QABarAbove` | ✅ available |
| Show Quick Access Toolbar Below Ribbon | `QABarBelow` | ✅ available |
| Snapshot | `snapshot` | 🟡 partial |
| Split Pages | `splitPages` | ✅ available |
| Substitute Fonts | `saf` | ⬜ planned |
| Text | `text` | ⬜ planned |
| Text Align | `textAlign` | 🟡 partial |
| Text Block | `textBlock` | ⬜ planned |
| Text Case | `textCase` | ⬜ planned |
| Text Direction | `textDirection` | ⬜ planned |
| Text List | `textList` | ⬜ planned |
| Text Para | `textPara` | ⬜ planned |
| Text Style | `textStyle` | 🟡 partial |
| Thickness Scale | `thicknessScale` | ⬜ planned |
| Ungroup | `ungroup` | ✅ available |
| Volume | `volume` | ✅ available |
| Widget | `widget` | ✅ available |

### View

| Command | Id | Status |
|---|---|---|
| Go To | `view.goto` | ✅ available |
| Zoom | `view.zoom` | ✅ available |
| Zoom In | `zoomIn` | ✅ available |
| Zoom Out | `zoomOut` | ✅ available |
| Page Layout | `view.layout` | ✅ available |
| Rotate View | `view.rotate` | ✅ available |
| Full Screen | `view.fullScreen` | ✅ available |
| Thumbnails | `view.pageThumbnails` | ✅ available |
| Bookmarks | `view.bookmarks` | ✅ available |
| Comments | `view.comments` | ✅ available |
| Attachments | `view.attachments` | ✅ available |
| Layers | `view.layers` | ✅ available |
| Fields | `view.fields` | ✅ available |
| Links | `view.links` | ✅ available |
| Named Destinations | `view.destinations` | ✅ available |
| Signatures | `view.signatures` | ✅ available |
| Search | `view.search` | ✅ available |
| Properties Pane | `view.properties` | ✅ available |
| Other Panes | `view.otherPanes` | ⬜ planned |
| Rulers | `view.showRulers` | ✅ available |
| Grid | `view.showGrid` | ✅ available |
| Guides | `view.showGuides` | ✅ available |
| Snap | `view.snap` | ✅ available |
| Dark Page Mode | `view.darkPageMode` | ✅ available |
| Toolbars | `view.toolbars` | ✅ available |
| UI Theme | `view.theme` | ✅ available |
| UI Options | `view.uiOptions` | ✅ available |
| 3D Model Tree | `3DModelTreeView` | ⬜ planned |
| 3D Model Tree | `view.3DModelTree` | ⬜ planned |
| AI Assistant | `aiView` | ⬜ planned |
| Accessibility Checker | `accCheckerView` | ✅ available |
| Accessibility Checker | `view.accChecker` | ✅ available |
| Accessibility Report | `accReportView` | ✅ available |
| Accessibility Report | `view.accReport` | ✅ available |
| Articles | `view.articles` | ⬜ planned |
| Attachments Pane | `attachmentsView` | ✅ available |
| Bookmarks Pane | `bookmarksView` | 🟡 partial |
| Checker | `chekerView` | ✅ available |
| Classic UI | `view.switchToClassicUI` | ✅ available |
| Collapse All | `collapseAll` | ✅ available |
| Comment Styles Palette | `view.commentStyles` | ✅ available |
| Comments View | `commentsView` | ✅ available |
| Compare Documents | `cmpDocs` | 🟡 partial |
| Comparison | `comparisonView` | ✅ available |
| Content | `view.content` | ✅ available |
| Content Pane | `contentsView` | ✅ available |
| Document Info Bar | `view.docInfoBar` | ✅ available |
| Document Recovery | `view.recovery` | ✅ available |
| Document Tabs | `view.docTabsGroup` | ⬜ planned |
| Exit Full Screen | `view.fullScreenExit` | ✅ available |
| Expand All | `expandAll` | ✅ available |
| Expand All Descendants | `expandAllDescendants` | ✅ available |
| Fields Pane | `fieldsView` | ✅ available |
| Highlight | `highlight` | ✅ available |
| Highlight Links | `view.highlightLinks` | ✅ available |
| History | `view.history` | ✅ available |
| Inertial Scroll | `inertialScroll` | ⬜ planned |
| Invert Vertical Axis | `view.mathY` | ✅ available |
| JavaScript Console | `jsConsole` | ✅ available |
| JavaScript Options… | `view.javaScriptOpts` | ✅ available |
| Layers Pane | `layersView` | 🟡 partial |
| Links Pane | `linksView` | 🟡 partial |
| Lock All Cmd Bars | `view.lockAllCmdBars` | ⬜ planned |
| Lock All Cmd Panes | `view.lockAllCmdPanes` | ⬜ planned |
| Lock Unlock All Cmd Bars | `view.lockUnlockAllCmdBars` | ⬜ planned |
| Lock Unlock All Cmd Panes | `view.lockUnlockAllCmdPanes` | ⬜ planned |
| Loupe | `loupeView` | 🟡 partial |
| Loupe | `view.loupe` | ✅ available |
| Mouse-Optimized UI | `view.switchToMouseUI` | ✅ available |
| Named Destinations Pane | `pdfNamedDestsView` | ✅ available |
| Order | `view.order` | ✅ available |
| Order Pane | `orderView` | ✅ available |
| Order Pane | `orderview` | ✅ available |
| Other Tools | `view.otherToolsGroup` | ⬜ planned |
| Override Document Colors | `view.overrideDocumentColors` | ✅ available |
| Page | `view.pageNumber` | ✅ available |
| Page Layout | `view.pageLayoutTypesGroup` | ⬜ planned |
| Page Size | `view.pageMeasureInfo` | ✅ available |
| Page Size Popup | `view.showPageSizePopup` | ✅ available |
| Pan and Zoom | `panZoomView` | ⬜ planned |
| Pan and Zoom | `view.panzoom` | ✅ available |
| Panes | `view.panesGroup` | ⬜ planned |
| Portfolio | `view.portfolio` | 🟡 partial |
| Portfolio Tools | `view.portfolioTools` | ✅ available |
| Recovery Pane | `recoveryView` | ⬜ planned |
| Restore Cmd Bars Visibility | `view.restoreCmdBarsVisibility` | ✅ available |
| Ribbon / Classic UI | `view.ribbonClassicUI` | ✅ available |
| Ribbon UI | `view.switchToRibbonUI` | ✅ available |
| Rotate View | `view.rotateViewGroup` | ⬜ planned |
| Rulers | `rulers` | ✅ available |
| Search Pane | `searchView` | ✅ available |
| Show | `view.show` | ⬜ planned |
| Show Page Size | `view.showPageMeasureInfo` | ✅ available |
| Show Pane Info Bar | `showPaneInfoBar` | ⬜ planned |
| Signatures Pane | `signaturesView` | ⬜ planned |
| Snap | `view.snapGroup` | ⬜ planned |
| Spell Check | `view.spellCheck` | ✅ available |
| Stamps Palette | `view.stamps` | ✅ available |
| Stamps Pane | `stampsView` | 🟡 partial |
| Tab Title Style | `tabTitleStyle` | ✅ available |
| Tags | `view.tags` | ✅ available |
| Tags Pane | `tagsView` | ✅ available |
| Tags Pane | `tagsview` | ✅ available |
| Thin Lines | `view.thinLines` | ⬜ planned |
| Thumbnails Pane | `thumbnailsView` | 🟡 partial |
| Toggle All Cmd Bars | `view.toggleAllCmdBars` | ✅ available |
| Toggle All Cmd Panes | `view.toggleAllCmdPanes` | ✅ available |
| Tool Names | `view.ribbonLabels` | ✅ available |
| Toolbar | `view.toolbar` | ✅ available |
| Touch-Optimized UI | `view.switchToTouchUI` | ✅ available |
| Touch/Mouse Mode | `view.touchMouseMode` | ✅ available |
| Transparency Grid | `view.showTranspGrid` | ✅ available |
| Unlock All Cmd Bars | `view.unlockAllCmdBars` | ⬜ planned |
| Unlock All Cmd Panes | `view.unlockAllCmdPanes` | ⬜ planned |
| XFA Tree | `view.xfaTree` | ⬜ planned |
| Zoom | `zoom` | ✅ available |
| Zoom Slider | `zoomSlider` | ⬜ planned |
| Zoom Tools | `zoomToolsGroup` | ⬜ planned |
| Zoom to Selection | `zoomSel` | ✅ available |

### Document

| Command | Id | Status |
|---|---|---|
| Insert Pages… | `document.insertPages` | ✅ available |
| Insert Blank Pages… | `document.insertEmptyPages` | ✅ available |
| Insert Images… | `document.insertImages` | ✅ available |
| Delete Pages… | `document.deletePages` | ✅ available |
| Extract Pages… | `document.extractPages` | ✅ available |
| Replace Pages… | `document.replacePages` | ✅ available |
| Move Pages… | `document.movePages` | ✅ available |
| Duplicate Pages… | `document.duplicatePages` | ✅ available |
| Swap Pages… | `document.swapPages` | ✅ available |
| Reverse Page Order | `document.reversePageOrder` | ✅ available |
| Merge Pages… | `document.mergePages` | ✅ available |
| Split | `document.splitGroup` | ✅ available |
| Rotate Pages… | `document.rotatePages` | ✅ available |
| Crop Pages… | `document.cropPages` | ✅ available |
| Resize Pages… | `document.resizePages` | ✅ available |
| Set Margins… | `document.setMargins` | ✅ available |
| Watermarks | `document.watermarks` | ✅ available |
| Background | `document.background` | ✅ available |
| Header and Footer | `document.headerAndFooter` | ✅ available |
| Bates Numbering | `document.batesNumbering` | ✅ available |
| Page Numbering… | `document.pageNumbering` | ✅ available |
| OCR Pages… | `document.OCRPages` | ✅ available |
| Recompress Images… | `document.recompressImages` | ✅ available |
| Replace Fonts… | `document.replaceFonts` | ⬜ planned |
| Digital Signatures | `document.signatures` | 🟡 partial |
| Security Policies | `securityPolicies` | ✅ available |
| Redaction | `document.redaction` | ✅ available |
| Document JavaScript | `document.javaScript` | ✅ available |
| Document Actions | `document.actions` | ✅ available |
| Word Count | `document.wordCount` | ✅ available |
| Accessibility | `acc` | ✅ available |
| Accessibility | `accessibility` | ✅ available |
| Action | `action` | ⬜ planned |
| Actions | `actions` | ⬜ planned |
| Add Bookmark | `addBookmark` | ✅ available |
| Add Image… | `document.addImage` | ✅ available |
| Add Link… | `addLink` | ✅ available |
| Annotations | `annots` | ✅ available |
| Apply All Redactions | `document.applyRedactions` | ✅ available |
| Audit Space Usage | `document.audit` | ✅ available |
| Bookmarks | `bookmarks` | ✅ available |
| Create Web Links from Barcodes | `createWebLinksFromBarcodes` | ✅ available |
| Create Web Links from URLs | `createWebLinksFromText` | ✅ available |
| Crop to White Margins | `document.cropToWhiteMargins` | ✅ available |
| Crypt Alter Bar | `cryptAlterBar` | ⬜ planned |
| Delete Empty Pages… | `document.deleteEmptyPages` | ✅ available |
| Detect Barcodes in Region… | `document.detectBarcodes` | ✅ available |
| Detect Barcodes… | `document.detectImageBarcode` | ✅ available |
| Digital Signature | `digisig` | ✅ available |
| Digital Signatures | `digiSigList` | ⬜ planned |
| Document Actions | `docActions` | ⬜ planned |
| Enable Editing (Discard PDF/A) | `document.discardPDFA` | ✅ available |
| Export All Bookmarks… | `document.exportBookmarks` | ✅ available |
| Export Selected Bookmarks… | `document.exportSelBookmarks` | ✅ available |
| Find | `document.find` | ✅ available |
| Find Duplicate Pages | `document.findDuplicates` | ✅ available |
| Highlight from File | `document.highlightFile` | ✅ available |
| Import Bookmarks | `document.importBookmarks` | ✅ available |
| Insert Pages from Clipboard… | `document.insertPagesFromClipboard` | ✅ available |
| Insert Rich Text… | `document.insertRTF` | ✅ available |
| Insert Scanned Pages… | `document.insertScannedPages` | ⬜ planned |
| Insert Text… | `document.insertText` | ✅ available |
| Java Script | `javaScript` | ✅ available |
| JavaScript Edit Snippets | `jsEditSnippets` | ✅ available |
| JavaScript List | `javaScriptList` | ✅ available |
| JavaScript Snippet | `jssnippet` | ✅ available |
| Layer Actions | `dao` | ⬜ planned |
| Link | `link` | ⬜ planned |
| More Page Tools | `document.pagesMore` | ⬜ planned |
| New Document from Selection… | `document.createNewDocFromSel` | ✅ available |
| New Stamp from Active Document | `document.createStampFromActiveDoc` | ✅ available |
| New Stamp from Selection… | `document.createStampFromSel` | ✅ available |
| Next Bookmark | `document.nextBookm` | ✅ available |
| Normalize Pages… | `document.normalizePages` | ✅ available |
| OCR Images… | `document.ocrImages` | ✅ available |
| OCR Image… | `OCRImage` | ✅ available |
| OCR Selected Region… | `document.OCRSelRgn` | ✅ available |
| Options… | `document.options` | ⬜ planned |
| Overlay | `overlay` | ✅ available |
| Overlay Pages… | `document.overlayPages` | ✅ available |
| Page Marks | `pagemarks` | 🟡 partial |
| Page Structure | `pageStructure` | ✅ available |
| Page Transitions | `document.pageTransitions` | ✅ available |
| Previous Bookmark | `document.prevBookm` | ✅ available |
| Public Key Security | `pubsec` | ✅ available |
| Rasterize Pages… | `document.rasterizePages` | ✅ available |
| Recompress Images | `recompressImages` | ⬜ planned |
| Recompress Image… | `document.recompressImage` | 🟡 partial |
| Redaction | `redaction` | ✅ available |
| Remove All Web Links | `removeAllWebLinks` | ✅ available |
| Remove Cropped Content | `document.removeCroppedContent` | 🟡 partial |
| Security Policies | `secPoliciesList` | ✅ available |
| Show Problem Details… | `document.showProblems` | ⬜ planned |
| Sign Markup | `signMarkup` | ✅ available |
| Signature | `document.signature` | ✅ available |
| Signs | `signs` | ✅ available |
| Spell Check and Replace | `document.spellCheckAndRepl` | ✅ available |
| Split Document… | `document.split` | ✅ available |
| Split Pages by Guides… | `document.splitPagesByGuides` | ✅ available |
| Split Pages… | `document.splitPages` | ✅ available |
| Standard Security Policy… | `standardSecPolicy` | ✅ available |
| Tags | `tags` | ✅ available |
| Web Links | `webLinksGroup` | ⬜ planned |

### Comments

| Command | Id | Status |
|---|---|---|
| Add Sticky Note | `addStickyNote` | ✅ available |
| Annotation | `tool.annot` | 🟡 partial |
| Add Comments | `comments.newGroup` | ⬜ planned |
| Show Comments | `comments.show` | ✅ available |
| Next Comment | `comments.next` | ✅ available |
| Previous Comment | `comments.previous` | ✅ available |
| Delete Comments | `comments.deleteGroup` | ⬜ planned |
| Flatten Comments | `comments.flattenGroup` | ✅ available |
| Import Comments… | `comments.import` | ✅ available |
| Export Comments to Data File… | `comments.export` | ✅ available |
| Summarize Comments… | `comments.summarize` | ✅ available |
| Comment Styles | `annotStylesList` | ⬜ planned |
| Comment Styles | `commentStyles` | ✅ available |
| 3D | `3d` | ⬜ planned |
| Accounting Calculator | `ac` | ✅ available |
| Accounting Calculator | `comments.ac` | ✅ available |
| Accounting Calculator | `comments.accountingCalc` | ✅ available |
| Add Annotation by Select | `addAnnotBySelect` | ✅ available |
| Add Annotations by Find Result | `addAnnotsByFindResult` | ✅ available |
| Add Annotations by Search Result | `addAnnotsBySearchResult` | ✅ available |
| Annotation | `annot` | 🟡 partial |
| Blend Mode | `blendMode` | ⬜ planned |
| Border Style | `borderStyle` | ✅ available |
| Border Width | `borderWidth` | ✅ available |
| Border Width Text | `borderWidthText` | ⬜ planned |
| Checked | `comments.checked` | ✅ available |
| Comment | `comment` | ⬜ planned |
| Comment Style | `annotStyle` | ✅ available |
| Delete All Comments | `comments.deleteAllDoc` | ✅ available |
| Delete All Visible Comments | `comments.deleteAllShown` | ✅ available |
| Delete Selected Comments | `comments.deleteSel` | ✅ available |
| Export Measures to CSV | `exportMeasuresToCSV` | ✅ available |
| Fill Color | `fcolor` | ✅ available |
| Flatten Comments… | `comments.flatten` | ✅ available |
| Flatten Selected | `comments.flattenSel` | ✅ available |
| Hide All Comments | `comments.hideAll` | ✅ available |
| Highlight Mode | `highlightMode` | ⬜ planned |
| Image | `image` | ⬜ planned |
| Line Cap | `lineCap` | ⬜ planned |
| Line End | `lineEnd` | ✅ available |
| Line Join | `lineJoin` | ⬜ planned |
| Line Start | `lineStart` | ✅ available |
| Locked | `comments.locked` | ✅ available |
| Macro | `macro` | 🟡 partial |
| Measure | `measure` | ✅ available |
| Measure Edit | `measureEdit` | ⬜ planned |
| Measure Info | `measureInfo` | ⬜ planned |
| Media | `media` | ✅ available |
| Media Annotation | `mediaAnnot` | ✅ available |
| Note Icon | `noteIcon` | ✅ available |
| Opacity | `opacity` | ✅ available |
| Printable | `comments.printable` | ✅ available |
| Selected Text | `textsel` | 🟡 partial |
| Selection | `selection` | ⬜ planned |
| Show All Comments | `comments.showAll` | ✅ available |
| Show/Hide All Comments | `comments.showhideAll` | ✅ available |
| Sound | `sound` | ✅ available |
| Stamp Designer | `stampCustom` | 🟡 partial |
| Stamps List | `stampsList` | ✅ available |
| Stroke Color | `scolor` | ✅ available |
| Stroke Opacity | `strokeOpacity` | ⬜ planned |
| Text Fill Color | `fcolorText` | ⬜ planned |
| Text Stroke Color | `scolorText` | ⬜ planned |
| Tool Styles List | `toolStylesList` | ⬜ planned |
| Underline Style | `underlineStyle` | ⬜ planned |

### Form

| Command | Id | Status |
|---|---|---|
| Edit Form | `tool.editFields` | ✅ available |
| Button | `btn` | ✅ available |
| Combobox | `combobox` | ✅ available |
| Edit | `tool.edit` | ⬜ planned |
| Field Editor | `editFields` | ✅ available |
| Forms | `forms` | 🟡 partial |
| Listbox | `listbox` | ✅ available |

### Tools

| Command | Id | Status |
|---|---|---|
| Basic Tools | `tools.basic` | ⬜ planned |
| Comment and Markup Tools | `tools.markup` | ⬜ planned |
| Measuring Tools | `tools.measure` | ⬜ planned |
| Content Editing Tools | `tools.editContent` | ⬜ planned |
| Link Tools | `tools.editLinks` | ⬜ planned |
| Media Tools | `tools.media` | ✅ available |
| Zoom Tools | `tools.zoom` | ⬜ planned |
| Compare Documents… | `compareDocs` | ✅ available |
| Launch Application | `launchExternalApp` | ✅ available |
| Allow/Block List | `bwList` | ⬜ planned |
| Metadata | `msp` | ✅ available |
| Other Tools | `otherToolsGroup` | ⬜ planned |
| Portfolio Schema | `portfolioSchemaList` | ✅ available |
| Quick Launch | `quickLaunch` | ✅ available |
| Selection Barcode Place | `selBarcodePlace` | ⬜ planned |
| Selection Image Place | `selImagePlace` | ✅ available |
| Tool | `tool` | 🟡 partial |

### Window

| Command | Id | Status |
|---|---|---|
| New Document Tab | `window.new` | ✅ available |
| Duplicate Tab | `duplicateTab` | ✅ available |
| Close Tab | `closeTab` | ✅ available |
| Close Other Tabs | `closeOtherTabs` | ✅ available |
| Close Tabs to the Left | `closeTabsToTheLeft` | ✅ available |
| Close Tabs to the Right | `closeTabsToTheRight` | ✅ available |
| Split | `window.split` | 🟡 partial |
| Remove Split | `window.removeSplit` | ✅ available |
| Distribute Tabs | `window.distributeTabs` | ⬜ planned |
| Horizontally | `window.distributeDocsHorz` | ⬜ planned |
| Merge All Tabs into One Group | `window.allDocsToOneTabGroup` | ⬜ planned |
| Merge All Tabs into This Window | `window.allDocsToActiveWindow` | ⬜ planned |
| Move Tab To | `window.moveTabTo` | ⬜ planned |
| Move Tabs… | `window.moveTabsTo` | ⬜ planned |
| Move to New Window | `window.activeDocToNewWindow` | ⬜ planned |
| New Blank Window | `window.newBlank` | ✅ available |
| New Horizontal Tab Group | `window.activeDocToNewHorzTabGroup` | ⬜ planned |
| New Vertical Tab Group | `window.activeDocToNewVertTabGroup` | ⬜ planned |
| Reset Document Panes Layout | `window.resetDocPanesLayout` | ✅ available |
| Reset Main Panes Layout | `window.resetMainPanesLayout` | ✅ available |
| Sort Tabs Alphabetically | `window.sortAllDocTabsAbc` | ✅ available |
| Vertically | `window.distributeDocsVert` | ⬜ planned |

### Help

| Command | Id | Status |
|---|---|---|
| User Guide | `help.contents` | ✅ available |
| Command-Line Help | `help.cmdline` | ✅ available |
| Project Website | `help.homepage` | ✅ available |
| Support Forum | `help.support` | ✅ available |
| Report a Problem… | `help.report` | ✅ available |
| Check for Updates | `help.checkForUpdates` | ✅ available |
| About Folitecta… | `help.about` | ✅ available |
| File Associations… | `help.fileAssoc` | ⬜ planned |

## Everything else (panes, dialogs, contextual tools)

| Command | Path | Status |
|---|---|---|
| `3DModelTreeView.options` | View › 3D Model Tree › Options | ⬜ planned |
| `ac.dateTime.hide` | Comments › Accounting Calculator › Date Time › Hide | ✅ available |
| `ac.dateTime.show` | Comments › Accounting Calculator › Date Time › Show | ✅ available |
| `ac.decimal.f` | Comments › Accounting Calculator › Decimal › Floating | ✅ available |
| `ac.decimal.five` | Comments › Accounting Calculator › Decimal › Five | ✅ available |
| `ac.decimal.four` | Comments › Accounting Calculator › Decimal › Four | ✅ available |
| `ac.decimal.one` | Comments › Accounting Calculator › Decimal › One | ✅ available |
| `ac.decimal.six` | Comments › Accounting Calculator › Decimal › Six | ✅ available |
| `ac.decimal.three` | Comments › Accounting Calculator › Decimal › Three | ✅ available |
| `ac.decimal.two` | Comments › Accounting Calculator › Decimal › Two | ✅ available |
| `ac.decimal.zero` | Comments › Accounting Calculator › Decimal › Zero | ✅ available |
| `ac.list.edit` | Comments › Accounting Calculator › List › Edit | ✅ available |
| `ac.list.edit.amount` | Comments › Accounting Calculator › List › Edit › Amount | ✅ available |
| `ac.list.edit.comment` | Comments › Accounting Calculator › List › Edit › Comment | ✅ available |
| `ac.list.edit.oper` | Comments › Accounting Calculator › List › Edit › Oper | ✅ available |
| `ac.list.invertSel` | Comments › Accounting Calculator › List › Invert Selection | ✅ available |
| `ac.list.selectAll` | Comments › Accounting Calculator › List › Select All | ✅ available |
| `ac.list.unselect` | Comments › Accounting Calculator › List › Unselect | ✅ available |
| `ac.preview.background` | Comments › Accounting Calculator › Preview › Background | ✅ available |
| `ac.preview.borderColor` | Comments › Accounting Calculator › Preview › Border Color | ✅ available |
| `ac.preview.borderStyle` | Comments › Accounting Calculator › Preview › Border Style | ✅ available |
| `ac.preview.borderWidth` | Comments › Accounting Calculator › Preview › Border Width | ✅ available |
| `ac.preview.font` | Comments › Accounting Calculator › Preview › Font | ✅ available |
| `ac.preview.reset` | Comments › Accounting Calculator › Preview › Reset | ✅ available |
| `ac.rounding.default` | Comments › Accounting Calculator › Rounding › Default | ✅ available |
| `ac.rounding.down` | Comments › Accounting Calculator › Rounding › Down | ✅ available |
| `ac.rounding.up` | Comments › Accounting Calculator › Rounding › Up | ✅ available |
| `ac.tape.reset` | Comments › Accounting Calculator › Tape › Reset | ✅ available |
| `action.goto.addCurrentView` | Document › Action › Go to › Add Current View | ⬜ planned |
| `action.goto.delete` | Document › Action › Go to › Delete | ⬜ planned |
| `action.goto.edit` | Document › Action › Go to › Edit | ⬜ planned |
| `action.goto.rename` | Document › Action › Go to › Rename | ⬜ planned |
| `action.goto.reset` | Document › Action › Go to › Reset | ⬜ planned |
| `action.goto.useCurrentView` | Document › Action › Go to › Use Current View | ⬜ planned |
| `action.goto.zoomIn` | Document › Action › Go to › Zoom in | ⬜ planned |
| `action.goto.zoomOut` | Document › Action › Go to › Zoom Out | ⬜ planned |
| `addAnnotsByFindResult.highlight` | Comments › Add Annotations by Find Result › Highlight | ✅ available |
| `addAnnotsByFindResult.strikeout` | Comments › Add Annotations by Find Result › Strikeout | ✅ available |
| `addAnnotsByFindResult.underline` | Comments › Add Annotations by Find Result › Underline | ✅ available |
| `addAnnotsBySearchResult.highlight` | Comments › Add Annotations by Search Result › Highlight | ✅ available |
| `addAnnotsBySearchResult.strikeout` | Comments › Add Annotations by Search Result › Strikeout | ✅ available |
| `addAnnotsBySearchResult.underline` | Comments › Add Annotations by Search Result › Underline | ✅ available |
| `aiView.clear` | View › AI Assistant › Clear | ⬜ planned |
| `aiView.summarizeDoc` | View › AI Assistant › Summarize Document | ⬜ planned |
| `aiView.translateDoc` | View › AI Assistant › Translate Document | ⬜ planned |
| `annot.3d` | Comments › Annotation › 3D | ⬜ planned |
| `annot.addText` | Comments › Annotation › Add Text | ⬜ planned |
| `annot.callout` | Comments › Annotation › Callout | ⬜ planned |
| `annot.copyText` | Comments › Annotation › Copy Text | ✅ available |
| `annot.deleteLabel` | Comments › Annotation › Delete Label | ⬜ planned |
| `annot.editLabel` | Comments › Annotation › Edit Label | ⬜ planned |
| `annot.line` | Comments › Annotation › Line | 🟡 partial |
| `annot.link` | Comments › Annotation › Link | ⬜ planned |
| `annot.measure` | Comments › Annotation › Measure | ✅ available |
| `annot.poly` | Comments › Annotation › Poly | ⬜ planned |
| `annot.popup` | Comments › Annotation › Popup | ✅ available |
| `annot.redact` | Comments › Annotation › Redact | ⬜ planned |
| `attachmentsView.autoShowPane` | View › Attachments Pane › Auto Show Pane | ✅ available |
| `attachmentsView.edit` | View › Attachments Pane › Edit | ✅ available |
| `attachmentsView.hdr` | View › Attachments Pane › Columns | ✅ available |
| `attachmentsView.hdr.compressed` | View › Attachments Pane › Columns › Compressed | ✅ available |
| `attachmentsView.hdr.description` | View › Attachments Pane › Columns › Description | ✅ available |
| `attachmentsView.hdr.location` | View › Attachments Pane › Columns › Location | ✅ available |
| `attachmentsView.hdr.modified` | View › Attachments Pane › Columns › Modified | ✅ available |
| `attachmentsView.hdr.name` | View › Attachments Pane › Columns › Name | ✅ available |
| `attachmentsView.hdr.relationship` | View › Attachments Pane › Columns › Relationship | ✅ available |
| `attachmentsView.hdr.size` | View › Attachments Pane › Columns › Size | ✅ available |
| `attachmentsView.hdr.type` | View › Attachments Pane › Columns › Type | ✅ available |
| `attachmentsView.new` | View › Attachments Pane › New | ✅ available |
| `attachmentsView.open` | View › Attachments Pane › Open | ✅ available |
| `attachmentsView.options` | View › Attachments Pane › Options | ✅ available |
| `attachmentsView.save` | View › Attachments Pane › Save | ✅ available |
| `batesNumberingFiles.hdr` | File › Bates Numbering Files › Columns | ⬜ planned |
| `batesNumberingFiles.hdr.location` | File › Bates Numbering Files › Columns › Location | ⬜ planned |
| `batesNumberingFiles.hdr.modified` | File › Bates Numbering Files › Columns › Modified | ⬜ planned |
| `batesNumberingFiles.hdr.pagesCount` | File › Bates Numbering Files › Columns › Pages Count | ⬜ planned |
| `batesNumberingFiles.hdr.pagesRange` | File › Bates Numbering Files › Columns › Pages Range | ⬜ planned |
| `batesNumberingFiles.hdr.pagesSize` | File › Bates Numbering Files › Columns › Pages Size | ⬜ planned |
| `batesNumberingFiles.hdr.size` | File › Bates Numbering Files › Columns › Size | ⬜ planned |
| `batesNumberingFiles.hdr.title` | File › Bates Numbering Files › Columns › Title | ⬜ planned |
| `batesNumberingFiles.hdr.type` | File › Bates Numbering Files › Columns › Type | ⬜ planned |
| `bookmarks.bold` | Document › Bookmarks › Bold | ✅ available |
| `bookmarks.italic` | Document › Bookmarks › Italic | ✅ available |
| `bookmarksView.addDest` | View › Bookmarks Pane › Add Destination | ✅ available |
| `bookmarksView.allInvalid` | View › Bookmarks Pane › All Invalid | ✅ available |
| `bookmarksView.autoExpandBookmarks` | View › Bookmarks Pane › Auto Expand Bookmarks | ✅ available |
| `bookmarksView.collapseAll` | View › Bookmarks Pane › Collapse All | ✅ available |
| `bookmarksView.createOptions` | View › Bookmarks Pane › Create Options | ✅ available |
| `bookmarksView.deleteOnlyChildren` | View › Bookmarks Pane › Delete Only Children | ✅ available |
| `bookmarksView.deleteWithoutChildren` | View › Bookmarks Pane › Delete Without Children | ✅ available |
| `bookmarksView.demote` | View › Bookmarks Pane › Demote | ✅ available |
| `bookmarksView.ensureVisibility` | View › Bookmarks Pane › Ensure Visibility | ✅ available |
| `bookmarksView.expandAll` | View › Bookmarks Pane › Expand All | ✅ available |
| `bookmarksView.goto` | View › Bookmarks Pane › Go to | ✅ available |
| `bookmarksView.importExport` | View › Bookmarks Pane › Import Export | ✅ available |
| `bookmarksView.markInvalid` | View › Bookmarks Pane › Mark Invalid | ✅ available |
| `bookmarksView.moveDown` | View › Bookmarks Pane › Move Down | ✅ available |
| `bookmarksView.moveUp` | View › Bookmarks Pane › Move Up | ✅ available |
| `bookmarksView.new` | View › Bookmarks Pane › New Bookmark | ✅ available |
| `bookmarksView.new.before` | View › Bookmarks Pane › New Bookmark › Before Current | ✅ available |
| `bookmarksView.new.firstChild` | View › Bookmarks Pane › New Bookmark › As First Child | ✅ available |
| `bookmarksView.new.lastChild` | View › Bookmarks Pane › New Bookmark › As Last Child | ✅ available |
| `bookmarksView.nextInvalid` | View › Bookmarks Pane › Next Invalid | ✅ available |
| `bookmarksView.options` | View › Bookmarks Pane › Options | ✅ available |
| `bookmarksView.pages` | View › Bookmarks Pane › Pages | ✅ available |
| `bookmarksView.pages.extract` | View › Bookmarks Pane › Pages › Extract | ✅ available |
| `bookmarksView.pages.print` | View › Bookmarks Pane › Pages › Print | ✅ available |
| `bookmarksView.pages.remove` | View › Bookmarks Pane › Pages › Remove | ✅ available |
| `bookmarksView.pages.select` | View › Bookmarks Pane › Pages › Select | ✅ available |
| `bookmarksView.prefs` | View › Bookmarks Pane › Preferences | ✅ available |
| `bookmarksView.promote` | View › Bookmarks Pane › Promote | ✅ available |
| `bookmarksView.resolveInvNDestsOnImport` | View › Bookmarks Pane › Resolve Inv N Destinations on Import | ⬜ planned |
| `bookmarksView.sections` | View › Bookmarks Pane › Sections | ✅ available |
| `bookmarksView.sections.extract` | View › Bookmarks Pane › Sections › Extract | ✅ available |
| `bookmarksView.sections.print` | View › Bookmarks Pane › Sections › Print | ✅ available |
| `bookmarksView.sections.remove` | View › Bookmarks Pane › Sections › Remove | ✅ available |
| `bookmarksView.sections.select` | View › Bookmarks Pane › Sections › Select | ✅ available |
| `bookmarksView.select` | View › Bookmarks Pane › Select | ✅ available |
| `bookmarksView.select.entireSection` | View › Bookmarks Pane › Select › Entire Section | ✅ available |
| `bookmarksView.select.targetPages` | View › Bookmarks Pane › Select › Target Pages | ✅ available |
| `bookmarksView.select.targetPagesE` | View › Bookmarks Pane › Select › Target Pages E | ✅ available |
| `bookmarksView.setDest` | View › Bookmarks Pane › Set Destination | ✅ available |
| `bookmarksView.setDestFromSel` | View › Bookmarks Pane › Set Destination from Selection | ✅ available |
| `bookmarksView.showContrastText` | View › Bookmarks Pane › Show Contrast Text | ✅ available |
| `bookmarksView.showIcons` | View › Bookmarks Pane › Show Icons | ✅ available |
| `bookmarksView.textSize` | View › Bookmarks Pane › Text Size | ✅ available |
| `bookmarksView.textSize.large` | View › Bookmarks Pane › Text Size › Large | ✅ available |
| `bookmarksView.textSize.medium` | View › Bookmarks Pane › Text Size › Medium | ✅ available |
| `bookmarksView.textSize.small` | View › Bookmarks Pane › Text Size › Small | ✅ available |
| `bookmarksView.useCurrentPage` | View › Bookmarks Pane › Use Current Page | ✅ available |
| `bookmarksView.useInheritedZoom` | View › Bookmarks Pane › Use Inherited Zoom | ✅ available |
| `bookmarksView.useRegionForDest` | View › Bookmarks Pane › Use Region for Destination | ✅ available |
| `bookmarksView.useTextForTitle` | View › Bookmarks Pane › Use Text for Title | ✅ available |
| `bookmarksView.viewOptions` | View › Bookmarks Pane › View Options | ✅ available |
| `bookmarksView.wrapTitles` | View › Bookmarks Pane › Wrap Titles | ✅ available |
| `bwList.addFile` | Tools › Allow/Block List › Add File | ⬜ planned |
| `bwList.addFolder` | Tools › Allow/Block List › Add Folder | ⬜ planned |
| `bwList.addHost` | Tools › Allow/Block List › Add Host | ⬜ planned |
| `bwList.addPattern` | Tools › Allow/Block List › Add Pattern | ⬜ planned |
| `chekerView.show.inComments` | View › Checker › Show › In Comments | ✅ available |
| `chekerView.show.inContent` | View › Checker › Show › In Content | ✅ available |
| `chekerView.show.inFields` | View › Checker › Show › In Fields | ✅ available |
| `chekerView.show.inLinks` | View › Checker › Show › In Links | ✅ available |
| `chekerView.show.inTags` | View › Checker › Show › In Tags | ✅ available |
| `cmpDocs.filter.all` | View › Compare Documents › Filter › All | ✅ available |
| `cmpDocs.filter.list` | View › Compare Documents › Filter › List | ✅ available |
| `cmpDocs.filter.none` | View › Compare Documents › Filter › None | ✅ available |
| `cmpDocs.goto.nextDiff` | View › Compare Documents › Go to › Next Diff | ✅ available |
| `cmpDocs.goto.prevDiff` | View › Compare Documents › Go to › Previous Diff | ✅ available |
| `cmpDocs.show.allDiffs` | View › Compare Documents › Show › All Diffs | ✅ available |
| `cmpDocs.show.both` | View › Compare Documents › Show › Both | 🟡 partial |
| `cmpDocs.show.new` | View › Compare Documents › Show › New | ✅ available |
| `cmpDocs.show.old` | View › Compare Documents › Show › Old | ✅ available |
| `cmpDocs.show.results` | View › Compare Documents › Show › Results | ✅ available |
| `cmpDocs.show.rowHighlights` | View › Compare Documents › Show › Row Highlights | ✅ available |
| `cmpDocs.show.summary` | View › Compare Documents › Show › Summary | ✅ available |
| `comments` | Comments | 🟡 partial |
| `comments.ac.closeAll` | Comments › Accounting Calculator › Close All | ✅ available |
| `comments.show.byAuthor.all` | Comments › Show Comments › By Author › All | ✅ available |
| `comments.show.byType.all` | Comments › Show Comments › By Type › All | ✅ available |
| `comments.show.checked.all` | Comments › Show Comments › By Check Mark › All | ✅ available |
| `comments.show.popups.closeAll` | Comments › Show Comments › Popups › Close All Popups | ✅ available |
| `comments.show.popups.openAll` | Comments › Show Comments › Popups › Open All Popups | ✅ available |
| `comments.show.popups.openNonEmpty` | Comments › Show Comments › Popups › Open All Non-Empty Popups | ✅ available |
| `comments.show.popups.opencloseAll` | Comments › Show Comments › Popups › Open/Close All Popups | ✅ available |
| `commentsView.collapseAll` | View › Comments View › Collapse All | ✅ available |
| `commentsView.expandAll` | View › Comments View › Expand All | ✅ available |
| `commentsView.exportSelected` | View › Comments View › Export Selected | ✅ available |
| `commentsView.groupBy` | View › Comments View › Group by | ✅ available |
| `commentsView.groupBy.author` | View › Comments View › Group by › Author | ✅ available |
| `commentsView.groupBy.color` | View › Comments View › Group by › Color | ✅ available |
| `commentsView.groupBy.crtDate` | View › Comments View › Group by › Created Date | ✅ available |
| `commentsView.groupBy.modDate` | View › Comments View › Group by › Modified Date | ✅ available |
| `commentsView.groupBy.page` | View › Comments View › Group by › Page | ✅ available |
| `commentsView.groupBy.state` | View › Comments View › Group by › State | ✅ available |
| `commentsView.groupBy.stateAndAuthor` | View › Comments View › Group by › State and Author | ✅ available |
| `commentsView.groupBy.stateAuthor` | View › Comments View › Group by › State Author | ✅ available |
| `commentsView.groupBy.subject` | View › Comments View › Group by › Subject | ✅ available |
| `commentsView.groupBy.submenu` | View › Comments View › Group by › Submenu | ✅ available |
| `commentsView.groupBy.type` | View › Comments View › Group by › Type | ✅ available |
| `commentsView.hide` | View › Comments View › Hide | ✅ available |
| `commentsView.hide.dates` | View › Comments View › Hide › Dates | ✅ available |
| `commentsView.next` | View › Comments View › Next | ✅ available |
| `commentsView.options` | View › Comments View › Options | ✅ available |
| `commentsView.popups` | View › Comments View › Popups | ✅ available |
| `commentsView.popups.close` | View › Comments View › Popups › Close | ✅ available |
| `commentsView.popups.open` | View › Comments View › Popups › Open | ✅ available |
| `commentsView.previous` | View › Comments View › Previous | ✅ available |
| `commentsView.show` | View › Comments View › Show | ✅ available |
| `commentsView.show.authors` | View › Comments View › Show › Authors | ✅ available |
| `commentsView.show.colors` | View › Comments View › Show › Colors | ✅ available |
| `commentsView.show.compactItems` | View › Comments View › Show › Compact Items | ✅ available |
| `commentsView.show.dates` | View › Comments View › Show › Dates | ✅ available |
| `commentsView.show.dynamicIcons` | View › Comments View › Show › Dynamic Icons | ✅ available |
| `commentsView.show.noTextPlaceholder` | View › Comments View › Show › No Text Placeholder | ✅ available |
| `commentsView.show.states` | View › Comments View › Show › States | ✅ available |
| `commentsView.show.subjects` | View › Comments View › Show › Subjects | ✅ available |
| `commentsView.show.text` | View › Comments View › Show › Show Text | ✅ available |
| `commentsView.show.uniformText` | View › Comments View › Show › Uniform Text | ✅ available |
| `commentsView.show.wrapLongTitles` | View › Comments View › Show › Wrap Long Titles | ✅ available |
| `commentsView.showCommentsFromHiddenLayers` | View › Comments View › Show Comments from Hidden Layers | ✅ available |
| `commentsView.showHiddenComments` | View › Comments View › Show Hidden Comments | ✅ available |
| `commentsView.showMenu` | View › Comments View › Show Menu | ✅ available |
| `contentsView.displaySelected` | View › Content Pane › Display Selected | ✅ available |
| `contentsView.options` | View › Content Pane › Options | ✅ available |
| `contentsView.showInTags` | View › Content Pane › Show in Tags | ✅ available |
| `cryptAlterBar.allow.always` | Document › Crypt Alter Bar › Allow › Always | ⬜ planned |
| `cryptAlterBar.allow.once` | Document › Crypt Alter Bar › Allow › Once | ⬜ planned |
| `dao.changeState.Off` | Document › Layer Actions › Change State › Off | ⬜ planned |
| `dao.changeState.On` | Document › Layer Actions › Change State › On | ⬜ planned |
| `dao.changeState.Toggle` | Document › Layer Actions › Change State › Toggle | ⬜ planned |
| `dao.usedData.AllOC` | Document › Layer Actions › Used Data › All Optional Content | ⬜ planned |
| `dao.usedData.Layers` | Document › Layer Actions › Used Data › Layers | ⬜ planned |
| `digisig.view.certify` | Document › Digital Signature › View › Certify | ✅ available |
| `digisig.view.clear` | Document › Digital Signature › View › Clear | ✅ available |
| `digisig.view.sign` | Document › Digital Signature › View › Sign | ✅ available |
| `digisig.view.summary` | Document › Digital Signature › View › Summary | ✅ available |
| `digisig.view.validate` | Document › Digital Signature › View › Validate | ✅ available |
| `document` | Document | 🟡 partial |
| `document.find.caseSensitive` | Document › Find › Case Sensitive | ✅ available |
| `document.find.coloredResults` | Document › Find › Colored Results | ✅ available |
| `document.find.gotoNext` | Document › Find › Go To Next | ✅ available |
| `document.find.gotoPrev` | Document › Find › Go To Previous | ✅ available |
| `document.find.ignoreDiacritics` | Document › Find › Ignore Diacritics | ✅ available |
| `document.find.includeAnnotations` | Document › Find › Include Annotations | ✅ available |
| `document.find.includeBookmarks` | Document › Find › Include Bookmarks | ✅ available |
| `document.find.includeExtLinks` | Document › Find › Include Extension Links | ✅ available |
| `document.find.includeFormFields` | Document › Find › Include Form Fields | ✅ available |
| `document.find.includePageText` | Document › Find › Include Page Text | ✅ available |
| `document.find.wholeWords` | Document › Find › Whole Words | ✅ available |
| `document.signatures.certify.invisible` | Document › Digital Signatures › Certify (Visible) › Certify (Invisible) | ✅ available |
| `dsd.pageSize` | Edit › Document Size › Page Size | ✅ available |
| `edit` | Edit | ✅ available |
| `editFields.items.add` | Form › Field Editor › Items › Add | ✅ available |
| `editFields.items.delete` | Form › Field Editor › Items › Delete | ✅ available |
| `editFields.items.moveDown` | Form › Field Editor › Items › Move Down | ✅ available |
| `editFields.items.moveUp` | Form › Field Editor › Items › Move Up | ✅ available |
| `editFields.items.sorted` | Form › Field Editor › Items › Sorted | ✅ available |
| `explorer.folder.back` | File › Explorer › Folder › Back | ✅ available |
| `explorer.folder.create` | File › Explorer › Folder › Create | ✅ available |
| `explorer.folder.forward` | File › Explorer › Folder › Forward | ✅ available |
| `explorer.folder.sync` | File › Explorer › Folder › Sync | ✅ available |
| `explorer.folder.up` | File › Explorer › Folder › Up | ✅ available |
| `explorer.places.nav` | File › Explorer › Places › Navigation | ✅ available |
| `explorer.places.nav.down` | File › Explorer › Places › Navigation › Down | ✅ available |
| `explorer.places.nav.up` | File › Explorer › Places › Navigation › Up | ✅ available |
| `explorer.thumb.loading` | File › Explorer › Thumbnail › Loading | ✅ available |
| `fieldsView.collapseAll` | View › Fields Pane › Collapse All | ✅ available |
| `fieldsView.expandAll` | View › Fields Pane › Expand All | ✅ available |
| `fieldsView.options` | View › Fields Pane › Options | ✅ available |
| `fieldsView.showFullNames` | View › Fields Pane › Show Full Names | ✅ available |
| `fieldsView.sort` | View › Fields Pane › Sort | ✅ available |
| `fieldsView.sort.alphabetic` | View › Fields Pane › Sort › Alphabetic | ✅ available |
| `fieldsView.sort.pages` | View › Fields Pane › Sort › Pages | ✅ available |
| `file` | File | ⬜ planned |
| `folderExts.fileList.loadAll` | File › Folder Filters › File List › Load All | ✅ available |
| `folderExts.fileList.saveAll` | File › Folder Filters › File List › Save All | ✅ available |
| `folderExts.fileList.saveSel` | File › Folder Filters › File List › Save Selection | ✅ available |
| `form` | Form | ⬜ planned |
| `forms.highlight.all` | Form › Forms › Highlight Form Fields › All | ✅ available |
| `forms.highlight.barcode` | Form › Forms › Highlight Form Fields › Barcode | ✅ available |
| `forms.highlight.checkBox` | Form › Forms › Highlight Form Fields › Check Box | ✅ available |
| `forms.highlight.comboBox` | Form › Forms › Highlight Form Fields › Combo Box | ✅ available |
| `forms.highlight.listBox` | Form › Forms › Highlight Form Fields › List Box | ✅ available |
| `forms.highlight.onlyrequired` | Form › Forms › Highlight Form Fields › Only Required Fields | ✅ available |
| `forms.highlight.pushButton` | Form › Forms › Highlight Form Fields › Push Button | ✅ available |
| `forms.highlight.radioButton` | Form › Forms › Highlight Form Fields › Radio Button | ✅ available |
| `forms.highlight.signature` | Form › Forms › Highlight Form Fields › Signature | ✅ available |
| `forms.highlight.textBox` | Form › Forms › Highlight Form Fields › Text Box | ✅ available |
| `forms.reset.selected` | Form › Forms › Reset Form › Reset Selected Fields | ✅ available |
| `highlight.destination` | View › Highlight › Destination | ✅ available |
| `historyList.clear` | File › History › Clear | ✅ available |
| `historyList.collapseAll` | File › History › Collapse All | ✅ available |
| `historyList.copyPath` | File › History › Copy Path | ✅ available |
| `historyList.expandAll` | File › History › Expand All | ✅ available |
| `historyList.folder` | File › History › Folder | ✅ available |
| `historyList.group` | File › History › Group | ✅ available |
| `historyList.group.show` | File › History › Group › Show | ✅ available |
| `historyList.open` | File › History › Open | ✅ available |
| `historyList.options` | File › History › Options | ✅ available |
| `historyList.options.showCaptions` | File › History › Options › Show Captions | ✅ available |
| `historyList.options.showPath` | File › History › Options › Show Path | ✅ available |
| `historyList.options.showPinned` | File › History › Options › Show Pinned | ✅ available |
| `historyList.options.showPreviewPopUp` | File › History › Options › Show Preview Pop Up | ✅ available |
| `historyList.options.showThumbnail` | File › History › Options › Show Thumbnail | ✅ available |
| `historyList.pin` | File › History › Pin | ✅ available |
| `historyList.remove` | File › History › Remove | ✅ available |
| `historyList.selectAll` | File › History › Select All | ✅ available |
| `historyList.sort` | File › History › Sort | ✅ available |
| `historyList.sort.alphabetical` | File › History › Sort › Alphabetical | ✅ available |
| `historyList.sort.closetime` | File › History › Sort › Closetime | ✅ available |
| `historyList.sort.modtime` | File › History › Sort › Modtime | ✅ available |
| `historyList.sort.opentime` | File › History › Sort › Opentime | ✅ available |
| `historyList.sortBy` | File › History › Sort by | ✅ available |
| `historyList.unpin` | File › History › Unpin | ✅ available |
| `imageEditor.convert.1bpp_indexed` | Edit › Image Editor › Convert › 1 bpp Indexed | ✅ available |
| `imageEditor.convert.24bpp` | Edit › Image Editor › Convert › 24 bpp | ✅ available |
| `imageEditor.convert.32bpp` | Edit › Image Editor › Convert › 32 bpp | ✅ available |
| `imageEditor.convert.8bpp_grayscale` | Edit › Image Editor › Convert › 8 bpp Grayscale | ✅ available |
| `imageEditor.convert.8bpp_indexed` | Edit › Image Editor › Convert › 8 bpp Indexed | ✅ available |
| `imageEditor.convert.grayscaleAlpha` | Edit › Image Editor › Convert › Grayscale Alpha | ✅ available |
| `imageEditor.convert.rcm` | Edit › Image Editor › Convert › Convert To | ✅ available |
| `imageEditor.filters.despeckle` | Edit › Image Editor › Filters › Despeckle | ✅ available |
| `imageEditor.filters.edgeDetect` | Edit › Image Editor › Filters › Edge Detect | ✅ available |
| `imageEditor.filters.embossAllDirections` | Edit › Image Editor › Filters › Emboss All Directions | ✅ available |
| `imageEditor.filters.embossHorzVert` | Edit › Image Editor › Filters › Emboss Horizontal Vertical | ✅ available |
| `imageEditor.filters.embossLaplascian` | Edit › Image Editor › Filters › Emboss Laplascian | ✅ available |
| `imageEditor.filters.embossLossy` | Edit › Image Editor › Filters › Emboss Lossy | ✅ available |
| `imageEditor.filters.gaussianBlur` | Edit › Image Editor › Filters › Gaussian Blur | ✅ available |
| `imageEditor.filters.meanRemoval` | Edit › Image Editor › Filters › Mean Removal | ✅ available |
| `imageEditor.filters.rcm` | Edit › Image Editor › Filters › Apply Filter | ✅ available |
| `imageEditor.filters.sharpen` | Edit › Image Editor › Filters › Sharpen | ✅ available |
| `imageEditor.filters.smooth` | Edit › Image Editor › Filters › Smooth | ✅ available |
| `imageEditor.zoom.combo` | Edit › Image Editor › Zoom › Zoom | ✅ available |
| `layersView.addNewGroup` | View › Layers Pane › Add New Group | ✅ available |
| `layersView.addNewLayer` | View › Layers Pane › Add New Layer | ✅ available |
| `layersView.applyOverrides` | View › Layers Pane › Apply Overrides | ✅ available |
| `layersView.extract` | View › Layers Pane › Extract | ✅ available |
| `layersView.extract.selected` | View › Layers Pane › Extract › Selected | ✅ available |
| `layersView.extract.visible` | View › Layers Pane › Extract › Visible | ✅ available |
| `layersView.flattenOC` | View › Layers Pane › Flatten Optional Content | ✅ available |
| `layersView.flattenSelected` | View › Layers Pane › Flatten Selected | ✅ available |
| `layersView.hideSelected` | View › Layers Pane › Hide Selected | ✅ available |
| `layersView.importAsLayer` | View › Layers Pane › Import as Layer | ✅ available |
| `layersView.merge` | View › Layers Pane › Merge | ✅ available |
| `layersView.mergeSelectedOnly` | View › Layers Pane › Merge Selected Only | ✅ available |
| `layersView.mergeSelectedWithChildren` | View › Layers Pane › Merge Selected with Children | ✅ available |
| `layersView.options` | View › Layers Pane › Options | ✅ available |
| `layersView.overrideByParent` | View › Layers Pane › Override by Parent | ✅ available |
| `layersView.resetOverrides` | View › Layers Pane › Reset Overrides | ✅ available |
| `layersView.select` | View › Layers Pane › Select | ✅ available |
| `layersView.selectCorresponding` | View › Layers Pane › Select Corresponding | 🟡 partial |
| `layersView.selectCorresponding.comments` | View › Layers Pane › Select Corresponding › Comments | 🟡 partial |
| `layersView.selectCorresponding.content` | View › Layers Pane › Select Corresponding › Content | ⬜ planned |
| `layersView.selectCorresponding.fields` | View › Layers Pane › Select Corresponding › Fields | 🟡 partial |
| `layersView.selectCorresponding.links` | View › Layers Pane › Select Corresponding › Links | 🟡 partial |
| `layersView.selectCorresponding.pages` | View › Layers Pane › Select Corresponding › Pages | ✅ available |
| `layersView.selectEmptyLayers` | View › Layers Pane › Select Empty Layers | ✅ available |
| `layersView.selectHiddenLayers` | View › Layers Pane › Select Hidden Layers | ✅ available |
| `layersView.selectLayers` | View › Layers Pane › Select Layers | ✅ available |
| `layersView.selectTextLabels` | View › Layers Pane › Select Text Labels | ✅ available |
| `layersView.selectVisibleLayers` | View › Layers Pane › Select Visible Layers | ✅ available |
| `layersView.setAsDefault` | View › Layers Pane › Set as Default | ✅ available |
| `layersView.setCurrentStatesAsDefault` | View › Layers Pane › Set Current States as Default | ✅ available |
| `layersView.showSelected` | View › Layers Pane › Show Selected | ✅ available |
| `layersView.sort` | View › Layers Pane › Sort | ✅ available |
| `layersView.sort.alphabetical` | View › Layers Pane › Sort › Alphabetical | ✅ available |
| `layersView.toggleSelected` | View › Layers Pane › Toggle Selected | ✅ available |
| `layersView.viewAllPages` | View › Layers Pane › View All Pages | ✅ available |
| `layersView.viewVisiblePages` | View › Layers Pane › View Visible Pages | ✅ available |
| `link.batch` | Document › Link › Batch Link | ⬜ planned |
| `linksView.import` | View › Links Pane › Import | ⬜ planned |
| `linksView.link` | View › Links Pane › Link | ✅ available |
| `linksView.link.copyURL` | View › Links Pane › Link › Copy URL | ✅ available |
| `linksView.link.openInBrowser` | View › Links Pane › Link › Open in Browser | ✅ available |
| `linksView.options` | View › Links Pane › Options | ⬜ planned |
| `linksView.wrapTitles` | View › Links Pane › Wrap Titles | ✅ available |
| `list.add` | Edit › List › Add | ⬜ planned |
| `list.clear` | Edit › List › Clear | ⬜ planned |
| `list.delete` | Edit › List › Delete | ⬜ planned |
| `list.down` | Edit › List › Down | ⬜ planned |
| `list.edit` | Edit › List › Edit | ⬜ planned |
| `list.up` | Edit › List › Up | ⬜ planned |
| `loupeView.showBoxes` | View › Loupe › Show Boxes | ⬜ planned |
| `loupeView.showGuides` | View › Loupe › Show Guides | ⬜ planned |
| `loupeView.snapshot` | View › Loupe › Snapshot | ⬜ planned |
| `loupeView.zoom` | View › Loupe › Zoom | ⬜ planned |
| `loupeView.zoom.combo` | View › Loupe › Zoom › Zoom | ⬜ planned |
| `loupeView.zoomIn` | View › Loupe › Zoom in | ✅ available |
| `loupeView.zoomOut` | View › Loupe › Zoom Out | ✅ available |
| `macro.LaunchApp.CurrentPage` | Comments › Macro › Launch Application › Current Page | ✅ available |
| `macro.LaunchApp.CurrentPage0` | Comments › Macro › Launch Application › Current Page 0 | ✅ available |
| `macro.LaunchApp.FileName` | Comments › Macro › Launch Application › File Name | ✅ available |
| `macro.LaunchApp.PositionX` | Comments › Macro › Launch Application › Position X | ✅ available |
| `macro.LaunchApp.PositionXpx` | Comments › Macro › Launch Application › Position Xpx | ✅ available |
| `macro.LaunchApp.PositionY` | Comments › Macro › Launch Application › Position Y | ✅ available |
| `macro.LaunchApp.PositionYpx` | Comments › Macro › Launch Application › Position Ypx | ✅ available |
| `macro.LaunchApp.ViewRotation` | Comments › Macro › Launch Application › View Rotation | ✅ available |
| `macro.LaunchApp.Zoom` | Comments › Macro › Launch Application › Zoom | ✅ available |
| `macro.bates.options` | Comments › Macro › Bates Number › Bates Numbering Settings | ✅ available |
| `macro.pageNumberingStyle.options` | Comments › Macro › Page Numbering Style › Options | ✅ available |
| `macro.rowData.options` | Comments › Macro › Row Data › Options | ⬜ planned |
| `media.position.Slider` | Comments › Media › Position › Position | ✅ available |
| `media.volume.Slider` | Comments › Media › Volume › Volume | ✅ available |
| `msp.hdr` | Tools › Metadata › Columns | ✅ available |
| `msp.hdr.tagName` | Tools › Metadata › Columns › Tag Name | ✅ available |
| `msp.hdr.value` | Tools › Metadata › Columns › Value | ✅ available |
| `noProps` | No Properties | ⬜ planned |
| `open.prefsPageDisplay` | File › Open › Preferences Page Display | ⬜ planned |
| `orderview.show.tableCells` | View › Order Pane › Show › Table Cells | ✅ available |
| `orderview.show.tablesAndFigures` | View › Order Pane › Show › Tables and Figures | ✅ available |
| `orderview.showmode.none` | View › Order Pane › Showmode › None | ✅ available |
| `orderview.showmode.order` | View › Order Pane › Showmode › Order | ✅ available |
| `orderview.showmode.type` | View › Order Pane › Showmode › Type | ✅ available |
| `orderview.tag.artifact` | View › Order Pane › Tag › Artifact | ✅ available |
| `orderview.tag.caption` | View › Order Pane › Tag › Caption | ✅ available |
| `orderview.tag.cell` | View › Order Pane › Tag › Cell | ✅ available |
| `orderview.tag.field` | View › Order Pane › Tag › Field | ✅ available |
| `orderview.tag.figure` | View › Order Pane › Tag › Figure | ✅ available |
| `orderview.tag.formula` | View › Order Pane › Tag › Formula | ✅ available |
| `orderview.tag.h1` | View › Order Pane › Tag › H 1 | ✅ available |
| `orderview.tag.h2` | View › Order Pane › Tag › H 2 | ✅ available |
| `orderview.tag.h3` | View › Order Pane › Tag › H 3 | ✅ available |
| `orderview.tag.h4` | View › Order Pane › Tag › H 4 | ✅ available |
| `orderview.tag.h5` | View › Order Pane › Tag › H 5 | ✅ available |
| `orderview.tag.h6` | View › Order Pane › Tag › H 6 | ✅ available |
| `orderview.tag.listItem` | View › Order Pane › Tag › List Item | ✅ available |
| `orderview.tag.note` | View › Order Pane › Tag › Note | ✅ available |
| `orderview.tag.p` | View › Order Pane › Tag › P | ✅ available |
| `orderview.tag.ref` | View › Order Pane › Tag › Ref | ✅ available |
| `orderview.tag.table` | View › Order Pane › Tag › Table | ✅ available |
| `panZoomView.snapshot` | View › Pan and Zoom › Snapshot | ⬜ planned |
| `pathedit.3point.arc` | Edit › Path Editing › 3 Point › Arc | ⬜ planned |
| `pathedit.3point.circle` | Edit › Path Editing › 3 Point › Circle | ⬜ planned |
| `pathedit.3point.rect` | Edit › Path Editing › 3 Point › Rect | ⬜ planned |
| `pathedit.anchors.add` | Edit › Path Editing › Anchors › Add | ⬜ planned |
| `pathedit.convert.corner` | Edit › Path Editing › Convert › Corner | ⬜ planned |
| `pathedit.convert.smooth` | Edit › Path Editing › Convert › Smooth | ⬜ planned |
| `pathedit.convert.straight` | Edit › Path Editing › Convert › Straight | ⬜ planned |
| `pdfNamedDestsView.createOptions` | View › Named Destinations Pane › Create Options | ✅ available |
| `pdfNamedDestsView.edit` | View › Named Destinations Pane › Edit | ✅ available |
| `pdfNamedDestsView.ensureVisibility` | View › Named Destinations Pane › Ensure Visibility | ✅ available |
| `pdfNamedDestsView.goto` | View › Named Destinations Pane › Go to | ✅ available |
| `pdfNamedDestsView.hdr` | View › Named Destinations Pane › Columns | ✅ available |
| `pdfNamedDestsView.hdr.details` | View › Named Destinations Pane › Columns › Details | ✅ available |
| `pdfNamedDestsView.hdr.name` | View › Named Destinations Pane › Columns › Name | ✅ available |
| `pdfNamedDestsView.hdr.page` | View › Named Destinations Pane › Columns › Page | ✅ available |
| `pdfNamedDestsView.hdr.type` | View › Named Destinations Pane › Columns › Type | ✅ available |
| `pdfNamedDestsView.new` | View › Named Destinations Pane › New | ✅ available |
| `pdfNamedDestsView.options` | View › Named Destinations Pane › Options | ✅ available |
| `pdfNamedDestsView.removeInvalid` | View › Named Destinations Pane › Remove Invalid | ✅ available |
| `pdfNamedDestsView.set` | View › Named Destinations Pane › Set | ✅ available |
| `pdfNamedDestsView.setFromSel` | View › Named Destinations Pane › Set from Selection | ✅ available |
| `pdfNamedDestsView.useCurrentPage` | View › Named Destinations Pane › Use Current Page | ✅ available |
| `pdfNamedDestsView.useInheritedZoom` | View › Named Destinations Pane › Use Inherited Zoom | ✅ available |
| `pdfNamedDestsView.useRegionForDest` | View › Named Destinations Pane › Use Region for Destination | ✅ available |
| `pdfNamedDestsView.useTextForTitle` | View › Named Destinations Pane › Use Text for Title | ✅ available |
| `pdfNamedDestsView.wrapTitles` | View › Named Destinations Pane › Wrap Titles | ✅ available |
| `position.Slider` | Edit › Position › Position | ⬜ planned |
| `presetList.add` | Edit › Presets › Add | ⬜ planned |
| `presetList.clone` | Edit › Presets › Clone | ⬜ planned |
| `presetList.delete` | Edit › Presets › Delete | ⬜ planned |
| `presetList.down` | Edit › Presets › Down | ⬜ planned |
| `presetList.edit` | Edit › Presets › Edit | ⬜ planned |
| `presetList.export` | Edit › Presets › Export | ⬜ planned |
| `presetList.import` | Edit › Presets › Import | ⬜ planned |
| `presetList.up` | Edit › Presets › Up | ⬜ planned |
| `presets.addNew` | Edit › Presets › Add New | ⬜ planned |
| `presets.addNew2` | Edit › Presets › Add New 2 | ⬜ planned |
| `presets.delete` | Edit › Presets › Delete | ⬜ planned |
| `presets.manage` | Edit › Presets › Manage | ⬜ planned |
| `props.applyDefault` | Edit › Properties › Apply Default | ✅ available |
| `props.makeDefault` | Edit › Properties › Make Default | ✅ available |
| `pubsec.cert.add` | Document › Public Key Security › Cert › Add | ✅ available |
| `pubsec.cert.browse` | Document › Public Key Security › Cert › Browse | ✅ available |
| `pubsec.cert.details` | Document › Public Key Security › Cert › Details | ✅ available |
| `pubsec.cert.remove` | Document › Public Key Security › Cert › Remove | ✅ available |
| `recoveryView.options` | View › Recovery Pane › Options | ⬜ planned |
| `recoveryView.recovery` | View › Recovery Pane › Recovery | ⬜ planned |
| `redaction.overlay.blur` | Document › Redaction › Overlay › Blur | ✅ available |
| `redaction.overlay.blurExtra` | Document › Redaction › Overlay › Blur Extra | ✅ available |
| `redaction.overlay.pixelate` | Document › Redaction › Overlay › Pixelate | ✅ available |
| `redaction.overlay.pixelateExtra` | Document › Redaction › Overlay › Pixelate Extra | ✅ available |
| `redaction.overlay.standard` | Document › Redaction › Overlay › Standard | ✅ available |
| `rulers.units.centimeters` | View › Rulers › Units › Centimeters | ✅ available |
| `rulers.units.inches` | View › Rulers › Units › Inches | ✅ available |
| `rulers.units.millimeters` | View › Rulers › Units › Millimeters | ✅ available |
| `rulers.units.picas` | View › Rulers › Units › Picas | ✅ available |
| `rulers.units.points` | View › Rulers › Units › Points | ✅ available |
| `search.annotateResults` | Edit › Search › Annotate Results | ⬜ planned |
| `search.coloredResults` | Edit › Search › Colored Results | ⬜ planned |
| `search.select` | Edit › Search › Select | ⬜ planned |
| `search.select.annotateResult` | Edit › Search › Select › Annotate Result | ⬜ planned |
| `search.start` | Edit › Search › Start | ✅ available |
| `search.stop` | Edit › Search › Stop | ⬜ planned |
| `searchView.caseSensitive` | View › Search Pane › Case Sensitive | ✅ available |
| `searchView.estimateFolder` | View › Search Pane › Estimate Folder | ✅ available |
| `searchView.ignoreDiacritics` | View › Search Pane › Ignore Diacritics | ✅ available |
| `searchView.includeAnnotations` | View › Search Pane › Include Annotations | ✅ available |
| `searchView.includeAttachments` | View › Search Pane › Include Attachments | ✅ available |
| `searchView.includeBookmarks` | View › Search Pane › Include Bookmarks | ✅ available |
| `searchView.includeDocInfo` | View › Search Pane › Include Document Info | ✅ available |
| `searchView.includeExtLinks` | View › Search Pane › Include Extension Links | ✅ available |
| `searchView.includeFormFields` | View › Search Pane › Include Form Fields | ✅ available |
| `searchView.includePageText` | View › Search Pane › Include Page Text | ✅ available |
| `searchView.lookInSubFolders` | View › Search Pane › Look in Sub Folders | ✅ available |
| `searchView.proximity` | View › Search Pane › Proximity | ✅ available |
| `searchView.proximity.adjacent` | View › Search Pane › Proximity › Adjacent | ✅ available |
| `searchView.proximity.sameDoc` | View › Search Pane › Proximity › Same Document | ✅ available |
| `searchView.proximity.samePage` | View › Search Pane › Proximity › Same Page | ✅ available |
| `searchView.proximity.samePara` | View › Search Pane › Proximity › Same Para | ✅ available |
| `searchView.results` | View › Search Pane › Results | ✅ available |
| `searchView.results.clear` | View › Search Pane › Results › Clear | ✅ available |
| `searchView.results.collapseAll` | View › Search Pane › Results › Collapse All | ✅ available |
| `searchView.results.expandAll` | View › Search Pane › Results › Expand All | ✅ available |
| `searchView.results.open` | View › Search Pane › Results › Open | ✅ available |
| `searchView.results.save` | View › Search Pane › Results › Save | ✅ available |
| `searchView.results.showCommentTypes` | View › Search Pane › Results › Show Comment Types | ✅ available |
| `searchView.results.showContextBookmarks` | View › Search Pane › Results › Show Context Bookmarks | ✅ available |
| `searchView.results.showIcon` | View › Search Pane › Results › Show Icon | ✅ available |
| `searchView.results.showPageNum` | View › Search Pane › Results › Show Page Num | ✅ available |
| `searchView.results.wrapText` | View › Search Pane › Results › Wrap Text | ✅ available |
| `searchView.wholeWords` | View › Search Pane › Whole Words | ✅ available |
| `selection.stamp` | Comments › Selection › Stamp | ⬜ planned |
| `sessions.clear` | File › Sessions › Clear | ✅ available |
| `sessions.export` | File › Sessions › Export | ✅ available |
| `sessions.import` | File › Sessions › Import | ✅ available |
| `sessions.open` | File › Sessions › Open | 🟡 partial |
| `sessions.open.addToActiveGroup` | File › Sessions › Open › Add to Active Group | 🟡 partial |
| `sessions.open.addToNewFrame` | File › Sessions › Open › Add to New Frame | ✅ available |
| `sessions.open.addToNewGroup` | File › Sessions › Open › Add to New Group | 🟡 partial |
| `sessions.open.addWholeSession` | File › Sessions › Open › Add Whole Session | ✅ available |
| `sessions.open.wholeSession` | File › Sessions › Open › Whole Session | ✅ available |
| `sessions.remove` | File › Sessions › Remove | ✅ available |
| `sessions.rename` | File › Sessions › Rename | ✅ available |
| `sessions.sort` | File › Sessions › Sort | ✅ available |
| `sessions.sort.alphabetical` | File › Sessions › Sort › Alphabetical | ✅ available |
| `sessions.sort.createTime` | File › Sessions › Sort › Create Time | ✅ available |
| `sessions.sort.openTime` | File › Sessions › Sort › Open Time | ✅ available |
| `sessions.sortBy` | File › Sessions › Sort by | ✅ available |
| `signMarkup.popup.createNew` | Document › Sign Markup › Popup › Create New | ✅ available |
| `signMarkup.popup.fromList` | Document › Sign Markup › Popup › From List | ✅ available |
| `signaturesView.options` | View › Signatures Pane › Options | ⬜ planned |
| `signs.markup.company` | Document › Signs › Placeholders › Company | ✅ available |
| `signs.markup.date` | Document › Signs › Placeholders › Date | ✅ available |
| `signs.markup.email` | Document › Signs › Placeholders › Email | ✅ available |
| `signs.markup.initials` | Document › Signs › Placeholders › Initials | ✅ available |
| `signs.markup.name` | Document › Signs › Placeholders › Name | ✅ available |
| `signs.markup.signature` | Document › Signs › Placeholders › Signature | ✅ available |
| `splitPages.removeLine.list` | Edit › Split Pages › Remove Line › List | ✅ available |
| `splitPages.removeLine.preview` | Edit › Split Pages › Remove Line › Preview | ✅ available |
| `stampCustom.add.curve` | Comments › Stamp Designer › Add › Curve | ✅ available |
| `stampCustom.add.field` | Comments › Stamp Designer › Add › Field | ✅ available |
| `stampCustom.add.field.counter` | Comments › Stamp Designer › Add › Field › Counter | ✅ available |
| `stampCustom.add.field.custom` | Comments › Stamp Designer › Add › Field › Custom | ✅ available |
| `stampCustom.add.field.date` | Comments › Stamp Designer › Add › Field › Date | ✅ available |
| `stampCustom.add.field.identity` | Comments › Stamp Designer › Add › Field › Identity | ✅ available |
| `stampCustom.add.field.identity.corporation` | Comments › Stamp Designer › Add › Field › Identity › Corporation | ✅ available |
| `stampCustom.add.field.identity.email` | Comments › Stamp Designer › Add › Field › Identity › Email | ✅ available |
| `stampCustom.add.field.identity.loginName` | Comments › Stamp Designer › Add › Field › Identity › Login Name | ✅ available |
| `stampCustom.add.field.identity.name` | Comments › Stamp Designer › Add › Field › Identity › Name | ✅ available |
| `stampCustom.add.field.macros` | Comments › Stamp Designer › Add › Field › Macros | ✅ available |
| `stampCustom.add.file` | Comments › Stamp Designer › Add › File | ✅ available |
| `stampCustom.add.line` | Comments › Stamp Designer › Add › Line | ✅ available |
| `stampCustom.add.oval` | Comments › Stamp Designer › Add › Oval | ✅ available |
| `stampCustom.add.polygon` | Comments › Stamp Designer › Add › Polygon | ✅ available |
| `stampCustom.add.polyline` | Comments › Stamp Designer › Add › Polyline | ✅ available |
| `stampCustom.add.rect` | Comments › Stamp Designer › Add › Rect | ✅ available |
| `stampCustom.add.template` | Comments › Stamp Designer › Add › Template | ✅ available |
| `stampCustom.add.text` | Comments › Stamp Designer › Add › Text | ✅ available |
| `stampCustom.manage.dateFormats` | Comments › Stamp Designer › Manage › Date Formats | ✅ available |
| `stampCustom.zoom.level` | Comments › Stamp Designer › Zoom › Level | ✅ available |
| `stampCustom.zoom.level.fit` | Comments › Stamp Designer › Zoom › Level › Fit | ✅ available |
| `stampsView.addCustom` | View › Stamps Pane › Add Custom | ✅ available |
| `stampsView.addGroup` | View › Stamps Pane › Add Group | ✅ available |
| `stampsView.addNew` | View › Stamps Pane › Add New | ✅ available |
| `stampsView.addNewFromCb` | View › Stamps Pane › Add New from Clipboard | ✅ available |
| `stampsView.getFromWeb` | View › Stamps Pane › Get from Web | ⬜ planned |
| `stampsView.options` | View › Stamps Pane › Options | ✅ available |
| `stampsView.sel` | View › Stamps Pane › Selection | ✅ available |
| `stampsView.sel.collection` | View › Stamps Pane › Selection › Collection | ✅ available |
| `stampsView.sel.collection.sort` | View › Stamps Pane › Selection › Collection › Sort | ✅ available |
| `stampsView.sel.stamp` | View › Stamps Pane › Selection › Stamp | ✅ available |
| `stampsView.sel.stamp.duplicate` | View › Stamps Pane › Selection › Stamp › Duplicate | ✅ available |
| `stampsView.selection` | View › Stamps Pane › Selection | ✅ available |
| `stampsView.selection.edit` | View › Stamps Pane › Selection › Edit | ✅ available |
| `stampsView.selection.recolor` | View › Stamps Pane › Selection › Recolor | ✅ available |
| `stampsView.virtual` | View › Stamps Pane › Virtual | ✅ available |
| `stampsView.virtual.favorites` | View › Stamps Pane › Virtual › Favorites | ✅ available |
| `stampsView.virtual.favorites.add` | View › Stamps Pane › Virtual › Favorites › Add | ✅ available |
| `stampsView.virtual.favorites.clear` | View › Stamps Pane › Virtual › Favorites › Clear | ✅ available |
| `stampsView.virtual.favorites.remove` | View › Stamps Pane › Virtual › Favorites › Remove | ✅ available |
| `stampsView.virtual.recents` | View › Stamps Pane › Virtual › Recents | ✅ available |
| `stampsView.virtual.recents.clear` | View › Stamps Pane › Virtual › Recents › Clear | ✅ available |
| `stampsView.virtual.recents.remove` | View › Stamps Pane › Virtual › Recents › Remove | ✅ available |
| `stampsView.zoomIn` | View › Stamps Pane › Zoom in | ✅ available |
| `stampsView.zoomOut` | View › Stamps Pane › Zoom Out | ✅ available |
| `tabTitleStyle.auto` | View › Tab Title Style › Auto | ✅ available |
| `tabTitleStyle.documentFilename` | View › Tab Title Style › Document Filename | ✅ available |
| `tabTitleStyle.documentTitle` | View › Tab Title Style › Document Title | ✅ available |
| `tabTitleStyle.endEllipsis` | View › Tab Title Style › End Ellipsis | ✅ available |
| `tabTitleStyle.midEllipsis` | View › Tab Title Style › Mid Ellipsis | ✅ available |
| `textPara.indent.decrease` | Edit › Text Para › Indent › Decrease | ⬜ planned |
| `textPara.indent.increase` | Edit › Text Para › Indent › Increase | ⬜ planned |
| `thumbnailsView.highlightVisible` | View › Thumbnails Pane › Highlight Visible | ✅ available |
| `thumbnailsView.interactiveVisible` | View › Thumbnails Pane › Interactive Visible | ⬜ planned |
| `thumbnailsView.logicalVisible` | View › Thumbnails Pane › Logical Visible | ✅ available |
| `thumbnailsView.options` | View › Thumbnails Pane › Options | ⬜ planned |
| `thumbnailsView.select` | View › Thumbnails Pane › Select | 🟡 partial |
| `thumbnailsView.select.bookmarks` | View › Thumbnails Pane › Select › Bookmarks | ✅ available |
| `thumbnailsView.select.comments` | View › Thumbnails Pane › Select › Comments | ✅ available |
| `thumbnailsView.select.empty` | View › Thumbnails Pane › Select › Empty | ✅ available |
| `thumbnailsView.select.fields` | View › Thumbnails Pane › Select › Fields | ✅ available |
| `thumbnailsView.select.images` | View › Thumbnails Pane › Select › Images | ✅ available |
| `thumbnailsView.select.landscape` | View › Thumbnails Pane › Select › Landscape | ✅ available |
| `thumbnailsView.select.linkTargets` | View › Thumbnails Pane › Select › Link Targets | ✅ available |
| `thumbnailsView.select.links` | View › Thumbnails Pane › Select › Links | ✅ available |
| `thumbnailsView.select.portrait` | View › Thumbnails Pane › Select › Portrait | ✅ available |
| `thumbnailsView.select.searchRes` | View › Thumbnails Pane › Select › Search Res | ✅ available |
| `thumbnailsView.select.shape` | View › Thumbnails Pane › Select › Shape | ⬜ planned |
| `thumbnailsView.select.text` | View › Thumbnails Pane › Select › Pages with Text | ✅ available |
| `thumbnailsView.select.with` | View › Thumbnails Pane › Select › Advanced Selection | ⬜ planned |
| `thumbnailsView.zoomIn` | View › Thumbnails Pane › Zoom in | ✅ available |
| `thumbnailsView.zoomOut` | View › Thumbnails Pane › Zoom Out | ✅ available |
| `tool.edit3d.poster.fromFile` | Tools › Tool › Edit 3 D › Poster › From File | ⬜ planned |
| `tool.edit3d.poster.fromView` | Tools › Tool › Edit 3 D › Poster › From View | ⬜ planned |
| `tool.editFields.mode.barcode` | Form › Tool › Edit Form › Field Type › Barcode | ⬜ planned |
| `tool.editFields.mode.button` | Form › Tool › Edit Form › Field Type › Button | ✅ available |
| `tool.editFields.mode.check` | Form › Tool › Edit Form › Field Type › Check | ✅ available |
| `tool.editFields.mode.combo` | Form › Tool › Edit Form › Field Type › Combo Box | ✅ available |
| `tool.editFields.mode.date` | Form › Tool › Edit Form › Field Type › Date | 🟡 partial |
| `tool.editFields.mode.image` | Form › Tool › Edit Form › Field Type › Image | ⬜ planned |
| `tool.editFields.mode.list` | Form › Tool › Edit Form › Field Type › List | ✅ available |
| `tool.editFields.mode.radio` | Form › Tool › Edit Form › Field Type › Radio | ✅ available |
| `tool.editFields.mode.select` | Form › Tool › Edit Form › Field Type › Select | ✅ available |
| `tool.editFields.mode.signature` | Form › Tool › Edit Form › Field Type › Signature | ✅ available |
| `tool.editFields.mode.text` | Form › Tool › Edit Form › Field Type › Text | ✅ available |
| `tool.link.openInBrowser.newTab` | Tools › Tool › Link › Open in Browser › New Tab | 🟡 partial |
| `tool.link.openInBrowser.newWindow` | Tools › Tool › Link › Open in Browser › New Window | 🟡 partial |
| `tool.sign` | Tools › Tool › Sign | ✅ available |
| `tool.sign.and` | Tools › Tool › Sign › And | ✅ available |
| `tool.table.cell.fill` | Tools › Tool › Create/Edit Tables › Cell › Fill | ✅ available |
| `tool.table.cell.merge` | Tools › Tool › Create/Edit Tables › Cell › Merge | ✅ available |
| `tool.table.cell.split` | Tools › Tool › Create/Edit Tables › Cell › Split | ✅ available |
| `tool.table.delete.columns` | Tools › Tool › Create/Edit Tables › Delete › Columns | ✅ available |
| `tool.table.delete.rows` | Tools › Tool › Create/Edit Tables › Delete › Rows | ✅ available |
| `tool.table.delete.table` | Tools › Tool › Create/Edit Tables › Delete › Table | ✅ available |
| `tool.table.insert.columns` | Tools › Tool › Create/Edit Tables › Insert › Columns | ✅ available |
| `tool.table.insert.columns.left` | Tools › Tool › Create/Edit Tables › Insert › Columns › Left | ✅ available |
| `tool.table.insert.columns.right` | Tools › Tool › Create/Edit Tables › Insert › Columns › Right | ✅ available |
| `tool.table.insert.rows` | Tools › Tool › Create/Edit Tables › Insert › Rows | ✅ available |
| `tool.table.insert.rows.above` | Tools › Tool › Create/Edit Tables › Insert › Rows › Above | ✅ available |
| `tool.table.insert.rows.below` | Tools › Tool › Create/Edit Tables › Insert › Rows › Below | ✅ available |
| `tool.table.line.color` | Tools › Tool › Create/Edit Tables › Line › Color | ✅ available |
| `view` | View | 🟡 partial |
| `view.portfolio.view.nav.next` | View › Portfolio › View › Navigation › Next | ✅ available |
| `view.portfolio.view.nav.prev` | View › Portfolio › View › Navigation › Previous | ✅ available |
| `view.portfolio.view.nav.up` | View › Portfolio › View › Navigation › Up | ✅ available |
| `volume.Slider` | Edit › Volume › Volume | ✅ available |
| `wdav.columns.created` | File › WebDAV › Columns › Created | ⬜ planned |
| `wdav.columns.extension` | File › WebDAV › Columns › Extension | ⬜ planned |
| `wdav.columns.modified` | File › WebDAV › Columns › Modified | ⬜ planned |
| `wdav.columns.size` | File › WebDAV › Columns › Size | ⬜ planned |
| `wdav.columns.sort` | File › WebDAV › Columns › Sort | ⬜ planned |
| `wdav.columns.sort.ascending` | File › WebDAV › Columns › Sort › Ascending | ⬜ planned |
| `wdav.columns.sort.descending` | File › WebDAV › Columns › Sort › Descending | ⬜ planned |
| `wdav.columns.title` | File › WebDAV › Columns › Title | ⬜ planned |
| `wdav.view.details` | File › WebDAV › View › Details | ⬜ planned |
| `wdav.view.extraLargeIcons` | File › WebDAV › View › Extra Large Icons | ⬜ planned |
| `wdav.view.largeIcons` | File › WebDAV › View › Large Icons | ⬜ planned |
| `wdav.view.mediumIcons` | File › WebDAV › View › Medium Icons | ⬜ planned |
| `wdav.view.modes` | File › WebDAV › View › View | ⬜ planned |
| `wdav.view.smallIcons` | File › WebDAV › View › Small Icons | ⬜ planned |
| `wdav.view.thumbnail` | File › WebDAV › View › Thumbnail | ⬜ planned |
| `wdav.view.tiles` | File › WebDAV › View › Tiles | ⬜ planned |
| `window.split.horz` | Window › Split › Split Horizontally | ✅ available |
| `window.split.spreadsheet` | Window › Split › Spreadsheet Split | ⬜ planned |
| `window.split.vert` | Window › Split › Split Vertically | ✅ available |
| `xmp.export.template` | File › XMP › Export › Template | ✅ available |
| `xmp.show.templates` | File › XMP › Show › Templates | ✅ available |
