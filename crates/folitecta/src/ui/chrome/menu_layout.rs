//! How the menus are laid out (FOL-L050): the commands used most at the top
//! of each menu of the menu bar, the others in submenus by subject, and
//! long submenus of the registry split the same way. What a menu (or a
//! submenu) holds and is not placed here goes into its More submenu, so
//! every command stays where it was reachable; the tests keep the menus
//! short as commands are added.

use folitecta_core::{Command, registry};

/// An entry of a layout.
#[derive(Debug, Clone, Copy)]
pub enum Item {
    /// A command (with its own submenu when it has one).
    Cmd(&'static str),
    Sep,
    /// A submenu of the layout: its label is `menu.group.<key>`.
    Group(&'static str, &'static [Item]),
    /// What the menu holds and is not placed above, in alphabetical order.
    More,
}

use Item::{Cmd, Group, More, Sep};

/// An entry as shown.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Cmd(&'static Command),
    Sep,
    Group { key: &'static str, nodes: Vec<Node> },
}

/// The menus of the menu bar.
pub const MENUS: &[(&str, &[Item])] = &[
    (
        "file",
        &[
            Cmd("open"),
            Cmd("recentFiles"),
            Group(
                "new",
                &[
                    Cmd("newDoc"),
                    Cmd("newDocBlank"),
                    Cmd("newDocTemplate"),
                    Sep,
                    Cmd("newDocFromFiles"),
                    Cmd("newDocImage"),
                    Cmd("newDocText"),
                    Cmd("newDocRTF"),
                    Cmd("newDocHtml"),
                    Cmd("newDocEmail"),
                    Cmd("newDocClipboard"),
                    Cmd("newDocSelection"),
                    Sep,
                    Cmd("newDocScanner"),
                    Cmd("newDocCustomScan"),
                    Sep,
                    Cmd("newDocCombine"),
                    Cmd("newDocumentList"),
                    Cmd("newPortfolio"),
                ],
            ),
            Sep,
            Cmd("save"),
            Cmd("saveAs"),
            Group(
                "saveOther",
                &[
                    Cmd("saveAsCopy"),
                    Cmd("saveCopyAs"),
                    Cmd("saveAll"),
                    Cmd("saveUnrestricted"),
                    Cmd("saveImage"),
                    Sep,
                    Cmd("saveAsGroup"),
                    Cmd("saveTo"),
                    Cmd("saveToComp"),
                    Cmd("saveToPlaces"),
                    Cmd("placesListSave"),
                    Cmd("browseForSave"),
                ],
            ),
            Cmd("revert"),
            Sep,
            Cmd("close"),
            Cmd("closeAll"),
            Sep,
            Group(
                "history",
                &[
                    Cmd("reopenJustClosedDoc"),
                    Cmd("sessions"),
                    Sep,
                    Cmd("historyList"),
                    Cmd("showHistory"),
                    Cmd("manageHistory"),
                    Cmd("saveSettingsAndHistory"),
                    Sep,
                    Cmd("sessionsList"),
                    Cmd("manageSessions"),
                    Cmd("saveCurrentSession"),
                    Cmd("saveCurrentSessionToFile"),
                    Cmd("openSessionFromFile"),
                ],
            ),
            Group(
                "fileOnDisk",
                &[
                    Cmd("renameDoc"),
                    Cmd("moveDoc"),
                    Cmd("moveToTrash"),
                    Sep,
                    Cmd("copySrcFile"),
                    Cmd("copySrcFileName"),
                    Cmd("openSrcFolder"),
                ],
            ),
            Sep,
            Cmd("export"),
            Cmd("document.exportOffice"),
            Group(
                "send",
                &[
                    Cmd("sendByEmail"),
                    Cmd("sendByEmailZip"),
                    Cmd("sendByEmailOpenAll"),
                    Cmd("sendByEmailOpenAllZip"),
                    Cmd("emailMethods"),
                    Sep,
                    Cmd("shareGroup"),
                    Cmd("esign"),
                    Cmd("document.esign"),
                    Cmd("wdav"),
                ],
            ),
            Sep,
            Cmd("print"),
            Cmd("printAll"),
            Sep,
            Cmd("docProps"),
            Cmd("document.security"),
            Group(
                "places",
                &[
                    Cmd("explorer"),
                    Cmd("folderExts"),
                    Cmd("addPlace"),
                    Sep,
                    Cmd("openFrom"),
                    Cmd("openGroup"),
                    Cmd("openFromComp"),
                    Cmd("openFromPlaces"),
                    Cmd("placesListOpen"),
                    Cmd("browseForOpen"),
                    Cmd("openFromURL"),
                ],
            ),
            Cmd("settings"),
            More,
            Sep,
            Cmd("exit"),
        ],
    ),
    (
        "edit",
        &[
            Cmd("undo"),
            Cmd("redo"),
            Sep,
            Cmd("cut"),
            Cmd("copy"),
            Cmd("paste"),
            Group(
                "paste",
                &[
                    Cmd("pasteSpecial"),
                    Sep,
                    Cmd("pastePlainText"),
                    Cmd("pasteRichText"),
                    Cmd("pasteImage"),
                    Cmd("pasteAnnots"),
                    Cmd("pasteBookmarks"),
                    Cmd("pasteFields"),
                    Cmd("pasteGuides"),
                    Cmd("pasteFiles"),
                    Cmd("pasteContents"),
                    Cmd("pasteMetafile"),
                    Cmd("pasteAsStamp"),
                    Sep,
                    Cmd("pastePages"),
                    Cmd("pastePagesSpecial"),
                ],
            ),
            Cmd("delete"),
            Sep,
            Cmd("selectAll"),
            Cmd("edit.deselect"),
            Group(
                "select",
                &[
                    Cmd("invertSelection"),
                    Cmd("selectPages"),
                    Cmd("selectTextItems"),
                    Cmd("selectImageItems"),
                    Cmd("selectShapes"),
                    Cmd("selectXFormItems"),
                    Cmd("selectContentItems"),
                    Cmd("selectAllContentItems"),
                    Cmd("selectionGroup"),
                    Sep,
                    Cmd("filterTextItems"),
                    Cmd("filterImageItems"),
                    Cmd("filterShapes"),
                    Cmd("filterContentItems"),
                    Sep,
                    Cmd("guidesSelectAll"),
                    Cmd("guidesSelectOnPage"),
                ],
            ),
            Sep,
            Cmd("find"),
            Cmd("findAndRepl"),
            Cmd("search"),
            Group(
                "pageText",
                &[
                    Cmd("tool.editText"),
                    Cmd("pageText.insert"),
                    Cmd("pageText.replace"),
                    Cmd("pageText.delete"),
                ],
            ),
            Sep,
            Group(
                "arrange",
                &[
                    Cmd("arrange"),
                    Cmd("order"),
                    Cmd("position"),
                    Sep,
                    Cmd("align"),
                    Cmd("alignto"),
                    Cmd("distribHorz"),
                    Cmd("distribVert"),
                    Cmd("distributeTo"),
                    Sep,
                    Cmd("makeSameWidth"),
                    Cmd("makeSameHeight"),
                    Cmd("makeSameSize"),
                    Sep,
                    Cmd("group"),
                    Cmd("ungroup"),
                    Cmd("addObjectGroup"),
                ],
            ),
            Group(
                "format",
                &[
                    Cmd("fontName"),
                    Cmd("fontSize"),
                    Cmd("font"),
                    Cmd("saf"),
                    Sep,
                    Cmd("text"),
                    Cmd("textStyle"),
                    Cmd("textAlign"),
                    Cmd("textPara"),
                    Cmd("textList"),
                    Cmd("textCase"),
                    Cmd("textDirection"),
                    Cmd("textBlock"),
                    Sep,
                    Cmd("lineWidthProfile"),
                    Cmd("thicknessScale"),
                    Cmd("fillRule"),
                    Cmd("inkSmoothLevel"),
                    Cmd("eraserDiameter"),
                ],
            ),
            Group(
                "pictures",
                &[
                    Cmd("editImage"),
                    Cmd("imageEditor"),
                    Cmd("replaceImage"),
                    Cmd("replaceImageClipboard"),
                    Cmd("replaceAllImageInstances"),
                    Cmd("removeClip"),
                    Sep,
                    Cmd("object"),
                    Cmd("editObject"),
                    Cmd("pathedit"),
                    Cmd("snapshot"),
                ],
            ),
            Group(
                "layers",
                &[Cmd("chooseLayer"), Cmd("defaultLayer"), Cmd("moveToNewLayer")],
            ),
            Sep,
            Cmd("props"),
            Group(
                "interface",
                &[
                    Cmd("customizeToolbars"),
                    Cmd("renameToolbar"),
                    Cmd("resetToolbar"),
                    Cmd("resetMenu"),
                    Cmd("QABarAbove"),
                    Cmd("QABarBelow"),
                    Sep,
                    Cmd("item"),
                    Cmd("list"),
                    Cmd("presets"),
                    Cmd("presetList"),
                ],
            ),
            Cmd("prefs"),
            More,
        ],
    ),
    (
        "view",
        &[
            Cmd("view.goto"),
            Cmd("zoomIn"),
            Cmd("zoomOut"),
            Cmd("zoomSel"),
            Group(
                "zoom",
                &[
                    Cmd("view.zoom"),
                    Cmd("zoom"),
                    Cmd("zoomSlider"),
                    Cmd("zoomToolsGroup"),
                ],
            ),
            Cmd("view.layout"),
            Cmd("view.rotate"),
            Cmd("view.fullScreen"),
            Sep,
            Cmd("view.reflow"),
            Cmd("view.readAloud"),
            Sep,
            Group(
                "panes",
                &[
                    Cmd("view.pageThumbnails"),
                    Cmd("view.bookmarks"),
                    Cmd("view.comments"),
                    Cmd("view.attachments"),
                    Cmd("view.layers"),
                    Cmd("view.fields"),
                    Cmd("view.links"),
                    Cmd("view.destinations"),
                    Cmd("view.signatures"),
                    Cmd("view.search"),
                    Cmd("view.properties"),
                    Cmd("view.history"),
                    Cmd("view.recovery"),
                    Cmd("view.spellCheck"),
                    Sep,
                    Cmd("view.stamps"),
                    Cmd("view.commentStyles"),
                ],
            ),
            Group(
                "accessibility",
                &[
                    Cmd("view.tags"),
                    Cmd("view.order"),
                    Cmd("view.content"),
                    Cmd("view.accChecker"),
                    Cmd("view.accReport"),
                    Sep,
                    Cmd("tagsView"),
                    Cmd("tagsview"),
                    Cmd("orderView"),
                    Cmd("orderview"),
                    Cmd("contentsView"),
                    Cmd("chekerView"),
                    Cmd("accCheckerView"),
                    Cmd("accReportView"),
                ],
            ),
            Group(
                "paneMenus",
                &[
                    Cmd("thumbnailsView"),
                    Cmd("bookmarksView"),
                    Cmd("commentsView"),
                    Cmd("attachmentsView"),
                    Cmd("layersView"),
                    Cmd("fieldsView"),
                    Cmd("linksView"),
                    Cmd("pdfNamedDestsView"),
                    Cmd("signaturesView"),
                    Cmd("searchView"),
                    Cmd("stampsView"),
                    Cmd("recoveryView"),
                    Sep,
                    Cmd("expandAll"),
                    Cmd("expandAllDescendants"),
                    Cmd("collapseAll"),
                ],
            ),
            Sep,
            Group(
                "rulers",
                &[
                    Cmd("view.showRulers"),
                    Cmd("rulers"),
                    Cmd("view.showGrid"),
                    Cmd("view.showGuides"),
                    Cmd("view.snap"),
                    Cmd("view.snapGroup"),
                    Cmd("view.mathY"),
                ],
            ),
            Group(
                "pageDisplay",
                &[
                    Cmd("view.darkPageMode"),
                    Cmd("view.greyPreview"),
                    Cmd("view.overrideDocumentColors"),
                    Cmd("view.showTranspGrid"),
                    Cmd("view.thinLines"),
                    Cmd("view.highlightLinks"),
                    Cmd("highlight"),
                    Sep,
                    Cmd("view.pageNumber"),
                    Cmd("view.pageMeasureInfo"),
                    Cmd("view.showPageMeasureInfo"),
                    Cmd("view.showPageSizePopup"),
                    Cmd("view.show"),
                    Sep,
                    Cmd("view.pageLayoutTypesGroup"),
                    Cmd("view.rotateViewGroup"),
                    Cmd("view.fullScreenExit"),
                ],
            ),
            Group(
                "tools",
                &[
                    Cmd("view.loupe"),
                    Cmd("loupeView"),
                    Cmd("view.panzoom"),
                    Cmd("panZoomView"),
                    Sep,
                    Cmd("cmpDocs"),
                    Cmd("comparisonView"),
                    Cmd("view.portfolio"),
                    Cmd("view.portfolioTools"),
                    Sep,
                    Cmd("jsConsole"),
                    Cmd("view.javaScriptOpts"),
                    Cmd("aiView"),
                    Cmd("3DModelTreeView"),
                ],
            ),
            Sep,
            Group(
                "interface",
                &[
                    Cmd("view.toolbars"),
                    Cmd("view.toolbar"),
                    Cmd("view.ribbonLabels"),
                    Cmd("view.theme"),
                    Cmd("view.uiOptions"),
                    Cmd("view.docInfoBar"),
                    Cmd("tabTitleStyle"),
                    Sep,
                    Cmd("view.ribbonClassicUI"),
                    Cmd("view.switchToRibbonUI"),
                    Cmd("view.switchToClassicUI"),
                    Cmd("view.switchToMouseUI"),
                    Cmd("view.switchToTouchUI"),
                    Cmd("view.touchMouseMode"),
                ],
            ),
            Group(
                "commandBars",
                &[
                    Cmd("view.restoreCmdBarsVisibility"),
                    Cmd("view.toggleAllCmdBars"),
                    Cmd("view.toggleAllCmdPanes"),
                    Sep,
                    Cmd("view.lockAllCmdBars"),
                    Cmd("view.lockAllCmdPanes"),
                    Cmd("view.unlockAllCmdBars"),
                    Cmd("view.unlockAllCmdPanes"),
                    Cmd("view.lockUnlockAllCmdBars"),
                    Cmd("view.lockUnlockAllCmdPanes"),
                ],
            ),
            More,
        ],
    ),
    (
        "document",
        &[
            Group(
                "insertPages",
                &[
                    Cmd("document.insertPages"),
                    Cmd("document.insertEmptyPages"),
                    Cmd("document.insertImages"),
                    Cmd("document.insertPagesFromClipboard"),
                    Cmd("document.insertScannedPages"),
                    Cmd("document.insertText"),
                    Cmd("document.insertRTF"),
                ],
            ),
            Cmd("document.deletePages"),
            Cmd("document.extractPages"),
            Cmd("document.replacePages"),
            Group(
                "arrangePages",
                &[
                    Cmd("document.movePages"),
                    Cmd("document.duplicatePages"),
                    Cmd("document.swapPages"),
                    Cmd("document.reversePageOrder"),
                    Cmd("document.mergePages"),
                    Sep,
                    Cmd("document.findDuplicates"),
                    Cmd("document.deleteEmptyPages"),
                    Cmd("document.createNewDocFromSel"),
                    Cmd("document.pagesMore"),
                ],
            ),
            Group(
                "split",
                &[
                    Cmd("document.split"),
                    Cmd("document.splitPages"),
                    Cmd("document.splitPagesByGuides"),
                    Cmd("document.splitGroup"),
                ],
            ),
            Sep,
            Cmd("document.rotatePages"),
            Group(
                "pageSize",
                &[
                    Cmd("document.cropPages"),
                    Cmd("document.cropToWhiteMargins"),
                    Cmd("document.removeCroppedContent"),
                    Cmd("document.resizePages"),
                    Cmd("document.setMargins"),
                    Cmd("document.normalizePages"),
                    Sep,
                    Cmd("document.overlayPages"),
                    Cmd("document.rasterizePages"),
                ],
            ),
            Group(
                "pageMarks",
                &[
                    Cmd("document.watermarks"),
                    Cmd("document.background"),
                    Cmd("document.headerAndFooter"),
                    Cmd("document.pageNumbering"),
                    Cmd("document.batesNumbering"),
                    Cmd("pagemarks"),
                    Sep,
                    Cmd("document.pageTransitions"),
                ],
            ),
            Sep,
            Group(
                "recognition",
                &[
                    Cmd("document.OCRPages"),
                    Cmd("document.ocrImages"),
                    Cmd("OCRImage"),
                    Cmd("document.OCRSelRgn"),
                    Cmd("document.ocrReview"),
                    Sep,
                    Cmd("document.cleanUpScans"),
                    Cmd("document.detectImageBarcode"),
                    Cmd("document.detectBarcodes"),
                ],
            ),
            Group(
                "optimize",
                &[
                    Cmd("document.recompressImages"),
                    Cmd("document.recompressImage"),
                    Cmd("recompressImages"),
                    Cmd("document.replaceFonts"),
                    Cmd("document.audit"),
                    Sep,
                    Cmd("document.preflight"),
                    Cmd("document.convertToPDFA"),
                    Cmd("document.convertToGrey"),
                    Cmd("document.discardPDFA"),
                    Sep,
                    Cmd("document.options"),
                    Cmd("document.showProblems"),
                ],
            ),
            Sep,
            Group(
                "navigation",
                &[
                    Cmd("bookmarks"),
                    Cmd("addBookmark"),
                    Cmd("document.nextBookm"),
                    Cmd("document.prevBookm"),
                    Cmd("document.bookmarksFromHeadings"),
                    Cmd("document.insertContents"),
                    Sep,
                    Cmd("document.importBookmarks"),
                    Cmd("document.exportBookmarks"),
                    Cmd("document.exportSelBookmarks"),
                ],
            ),
            Group(
                "links",
                &[
                    Cmd("addLink"),
                    Cmd("link"),
                    Cmd("createWebLinksFromText"),
                    Cmd("createWebLinksFromBarcodes"),
                    Cmd("removeAllWebLinks"),
                    Cmd("webLinksGroup"),
                ],
            ),
            Cmd("document.find"),
            Cmd("document.wordCount"),
            Cmd("document.spellCheckAndRepl"),
            Sep,
            Group(
                "protection",
                &[
                    Cmd("document.signatures"),
                    Cmd("digisig"),
                    Cmd("digiSigList"),
                    Cmd("document.signature"),
                    Cmd("signs"),
                    Cmd("signMarkup"),
                    Sep,
                    Cmd("securityPolicies"),
                    Cmd("secPoliciesList"),
                    Cmd("standardSecPolicy"),
                    Cmd("pubsec"),
                    Cmd("cryptAlterBar"),
                ],
            ),
            Group(
                "redaction",
                &[
                    Cmd("document.redaction"),
                    Cmd("redaction"),
                    Cmd("document.applyRedactions"),
                    Cmd("overlay"),
                ],
            ),
            Sep,
            Group(
                "scripts",
                &[
                    Cmd("document.javaScript"),
                    Cmd("javaScript"),
                    Cmd("javaScriptList"),
                    Cmd("jssnippet"),
                    Cmd("jsEditSnippets"),
                    Sep,
                    Cmd("document.actions"),
                    Cmd("docActions"),
                    Cmd("actions"),
                    Cmd("action"),
                    Cmd("dao"),
                ],
            ),
            Group(
                "accessibility",
                &[
                    Cmd("accessibility"),
                    Cmd("acc"),
                    Cmd("tags"),
                    Cmd("pageStructure"),
                ],
            ),
            More,
        ],
    ),
    (
        "comments",
        &[
            Cmd("addStickyNote"),
            Group(
                "addComments",
                &[
                    Cmd("tool.annot"),
                    Cmd("annot"),
                    Cmd("comments.newGroup"),
                    Cmd("comment"),
                    Sep,
                    Cmd("textsel"),
                    Cmd("selection"),
                    Cmd("addAnnotBySelect"),
                    Cmd("addAnnotsByFindResult"),
                    Cmd("addAnnotsBySearchResult"),
                    Cmd("image"),
                ],
            ),
            Sep,
            Cmd("comments.next"),
            Cmd("comments.previous"),
            Group(
                "showHide",
                &[
                    Cmd("comments.show"),
                    Cmd("comments.showAll"),
                    Cmd("comments.hideAll"),
                    Cmd("comments.showhideAll"),
                ],
            ),
            Group(
                "status",
                &[
                    Cmd("comments.checked"),
                    Cmd("comments.locked"),
                    Cmd("comments.printable"),
                ],
            ),
            Sep,
            Group(
                "deleteFlatten",
                &[
                    Cmd("comments.deleteSel"),
                    Cmd("comments.deleteAllShown"),
                    Cmd("comments.deleteAllDoc"),
                    Cmd("comments.deleteGroup"),
                    Sep,
                    Cmd("comments.flatten"),
                    Cmd("comments.flattenSel"),
                    Cmd("comments.flattenGroup"),
                ],
            ),
            Cmd("comments.import"),
            Cmd("comments.export"),
            Cmd("comments.summarize"),
            Sep,
            Group(
                "style",
                &[
                    Cmd("annotStyle"),
                    Cmd("commentStyles"),
                    Cmd("annotStylesList"),
                    Cmd("toolStylesList"),
                    Sep,
                    Cmd("scolor"),
                    Cmd("fcolor"),
                    Cmd("opacity"),
                    Cmd("strokeOpacity"),
                    Cmd("blendMode"),
                    Cmd("scolorText"),
                    Cmd("fcolorText"),
                    Cmd("noteIcon"),
                    Cmd("highlightMode"),
                ],
            ),
            Group(
                "lines",
                &[
                    Cmd("borderStyle"),
                    Cmd("borderWidth"),
                    Cmd("borderWidthText"),
                    Cmd("underlineStyle"),
                    Sep,
                    Cmd("lineStart"),
                    Cmd("lineEnd"),
                    Cmd("lineCap"),
                    Cmd("lineJoin"),
                ],
            ),
            Sep,
            Group(
                "measure",
                &[
                    Cmd("measure"),
                    Cmd("measureEdit"),
                    Cmd("measureInfo"),
                    Cmd("exportMeasuresToCSV"),
                ],
            ),
            Group("stamps", &[Cmd("stampCustom"), Cmd("stampsList")]),
            Group(
                "calculator",
                &[Cmd("comments.accountingCalc"), Cmd("ac"), Cmd("comments.ac")],
            ),
            Group(
                "media",
                &[Cmd("media"), Cmd("mediaAnnot"), Cmd("sound"), Cmd("3d")],
            ),
            More,
        ],
    ),
    (
        "tools",
        &[
            Cmd("tool"),
            Sep,
            Cmd("tools.actionSequences"),
            Cmd("compareDocs"),
            Cmd("quickLaunch"),
            Cmd("launchExternalApp"),
            Cmd("msp"),
            Cmd("portfolioSchemaList"),
            Sep,
            Cmd("selImagePlace"),
            Cmd("selBarcodePlace"),
            Cmd("bwList"),
            Group(
                "toolSets",
                &[
                    Cmd("tools.basic"),
                    Cmd("tools.markup"),
                    Cmd("tools.editContent"),
                    Cmd("tools.editLinks"),
                    Cmd("tools.measure"),
                    Cmd("tools.media"),
                    Cmd("tools.zoom"),
                    Cmd("otherToolsGroup"),
                ],
            ),
            More,
        ],
    ),
    (
        "window",
        &[
            Cmd("window.new"),
            Cmd("window.newBlank"),
            Cmd("window.activeDocToNewWindow"),
            Cmd("window.allDocsToActiveWindow"),
            Sep,
            Group(
                "tabs",
                &[
                    Cmd("closeTab"),
                    Cmd("closeOtherTabs"),
                    Cmd("closeTabsToTheLeft"),
                    Cmd("closeTabsToTheRight"),
                    Sep,
                    Cmd("duplicateTab"),
                    Cmd("window.sortAllDocTabsAbc"),
                ],
            ),
            Group(
                "tabGroups",
                &[
                    Cmd("window.activeDocToNewVertTabGroup"),
                    Cmd("window.activeDocToNewHorzTabGroup"),
                    Cmd("window.moveTabTo"),
                    Cmd("window.moveTabsTo"),
                    Sep,
                    Cmd("window.distributeDocsVert"),
                    Cmd("window.distributeDocsHorz"),
                    Cmd("window.distributeTabs"),
                    Cmd("window.allDocsToOneTabGroup"),
                ],
            ),
            Sep,
            Cmd("window.split"),
            Cmd("window.removeSplit"),
            Sep,
            Cmd("window.resetMainPanesLayout"),
            Cmd("window.resetDocPanesLayout"),
            More,
        ],
    ),
];

/// Long submenus of the registry, by the id of the command that holds them.
pub const SUBMENUS: &[(&str, &[Item])] = &[
    (
        "tool",
        &[
            Cmd("tool.hand"),
            Cmd("tool.selectText"),
            Cmd("tool.select"),
            Cmd("tool.zoom"),
            Cmd("tool.zoomDyn"),
            Cmd("tool.loupe"),
            Cmd("tool.panZoom"),
            Cmd("tool.snapshot"),
            Sep,
            Cmd("tool.annot"),
            Cmd("tool.editAnnots"),
            Cmd("tool.textCorrectionMarkup"),
            Sep,
            Group(
                "addContent",
                &[
                    Cmd("tool.addText"),
                    Cmd("tool.selImagePlace"),
                    Cmd("tool.selBarcodePlace"),
                    Cmd("tool.barcodeType"),
                    Cmd("tool.table"),
                    Cmd("tool.link"),
                    Cmd("tool.selAnnotPlace"),
                ],
            ),
            Group(
                "editContent",
                &[
                    Cmd("tool.editText"),
                    Cmd("tool.editContent"),
                    Cmd("tool.edit"),
                    Cmd("tool.edit3d"),
                    Cmd("tool.editFields"),
                    Cmd("tool.cropPage"),
                    Cmd("tool.selPageRegions"),
                    Cmd("tool.order"),
                ],
            ),
            Group(
                "signatures",
                &[
                    Cmd("tool.digisig"),
                    Cmd("tool.signMarkup"),
                    Cmd("tool.freeSign"),
                    Cmd("tool.sign"),
                ],
            ),
            Group(
                "measure",
                &[
                    Cmd("tool.calibrate"),
                    Cmd("tool.clearMeasure"),
                    Cmd("tool.convertToMeasure"),
                    Cmd("tool.convertLinesToPolyline"),
                    Cmd("tool.convertPolyToLines"),
                    Cmd("tool.convertToPolygon"),
                ],
            ),
            Group(
                "toolOptions",
                &[
                    Cmd("tool.keepSelected"),
                    Cmd("tool.exclusiveMode"),
                    Cmd("tool.fitBoxByTextContent"),
                ],
            ),
            More,
        ],
    ),
    (
        "tool.annot",
        &[
            Group(
                "notesText",
                &[
                    Cmd("tool.annot.stickyNote"),
                    Cmd("tool.annot.textBox"),
                    Cmd("tool.annot.typeWriter"),
                    Cmd("tool.annot.callout"),
                    Cmd("tool.annot.insertText"),
                    Cmd("tool.annot.replaceText"),
                ],
            ),
            Group(
                "textMarkup",
                &[
                    Cmd("tool.annot.highlight"),
                    Cmd("tool.annot.underline"),
                    Cmd("tool.annot.strikeout"),
                    Cmd("tool.annot.highlightArea"),
                    Cmd("tool.annot.freeHighlight"),
                ],
            ),
            Group(
                "drawing",
                &[
                    Cmd("tool.annot.pencil"),
                    Cmd("tool.annot.eraser"),
                    Sep,
                    Cmd("tool.annot.line"),
                    Cmd("tool.annot.arrow"),
                    Cmd("tool.annot.square"),
                    Cmd("tool.annot.circle"),
                    Cmd("tool.annot.polygon"),
                    Cmd("tool.annot.polyline"),
                    Cmd("tool.annot.cloud"),
                ],
            ),
            Group(
                "measure",
                &[
                    Cmd("tool.annot.distance"),
                    Cmd("tool.annot.perimeter"),
                    Cmd("tool.annot.area"),
                    Cmd("tool.annot.angle"),
                    Cmd("tool.annot.radius"),
                    Cmd("tool.annot.volume"),
                ],
            ),
            Sep,
            Cmd("tool.annot.stamp"),
            Cmd("tool.annot.tickMark"),
            Cmd("tool.annot.fileAttachment"),
            Cmd("tool.annot.redaction"),
            Group(
                "mediaLinks",
                &[
                    Cmd("tool.annot.editLink"),
                    Cmd("tool.annot.sound"),
                    Cmd("tool.annot.editSound"),
                    Cmd("tool.annot.editVideo"),
                    Cmd("tool.annot.edit3D"),
                ],
            ),
            More,
        ],
    ),
    (
        "macro",
        &[
            Cmd("macro.guide"),
            Sep,
            Group(
                "macroPage",
                &[
                    Cmd("macro.page"),
                    Cmd("macro.pages"),
                    Cmd("macro.pageLabel"),
                    Cmd("macro.pageNumberingStyle"),
                    Cmd("macro.srcPages"),
                    Cmd("macro.autonumber"),
                ],
            ),
            Group(
                "macroDate",
                &[
                    Cmd("macro.date"),
                    Cmd("macro.time"),
                    Sep,
                    Cmd("macro.year"),
                    Cmd("macro.month"),
                    Cmd("macro.day"),
                    Cmd("macro.hour"),
                    Cmd("macro.minute"),
                    Cmd("macro.seconds"),
                ],
            ),
            Group(
                "macroDocument",
                &[
                    Cmd("macro.docTitle"),
                    Cmd("macro.docInfo"),
                    Cmd("macro.bookmarkTitle"),
                    Cmd("macro.fileName"),
                    Cmd("macro.fileExt"),
                    Cmd("macro.docnumber"),
                    Cmd("macro.imagenumber"),
                ],
            ),
            Group(
                "macroFolder",
                &[
                    Cmd("macro.folderName"),
                    Cmd("macro.folderPath"),
                    Cmd("macro.folderTitle"),
                ],
            ),
            Group("macroBates", &[Cmd("macro.bates"), Cmd("macro.batesValue")]),
            Group(
                "macroSystem",
                &[
                    Cmd("macro.user"),
                    Cmd("macro.computer"),
                    Cmd("macro.appVersion"),
                    Cmd("macro.env"),
                    Cmd("macro.region"),
                    Cmd("macro.LaunchApp"),
                ],
            ),
            Group(
                "macroRows",
                &[
                    Cmd("macro.rowData"),
                    Cmd("macro.rowDataIndex"),
                    Cmd("macro.rowDataName"),
                    Cmd("macro.rowIndex"),
                ],
            ),
            More,
        ],
    ),
    (
        "bookmarksView",
        &[
            Cmd("bookmarksView.new"),
            Cmd("bookmarksView.goto"),
            Cmd("bookmarksView.setDest"),
            Cmd("bookmarksView.setDestFromSel"),
            Cmd("bookmarksView.addDest"),
            Sep,
            Group(
                "arrange",
                &[
                    Cmd("bookmarksView.promote"),
                    Cmd("bookmarksView.demote"),
                    Cmd("bookmarksView.moveUp"),
                    Cmd("bookmarksView.moveDown"),
                    Sep,
                    Cmd("bookmarksView.expandAll"),
                    Cmd("bookmarksView.collapseAll"),
                    Cmd("bookmarksView.select"),
                    Cmd("bookmarksView.pages"),
                    Cmd("bookmarksView.sections"),
                    Sep,
                    Cmd("bookmarksView.deleteOnlyChildren"),
                    Cmd("bookmarksView.deleteWithoutChildren"),
                ],
            ),
            Group(
                "invalid",
                &[
                    Cmd("bookmarksView.allInvalid"),
                    Cmd("bookmarksView.nextInvalid"),
                    Cmd("bookmarksView.markInvalid"),
                    Cmd("bookmarksView.resolveInvNDestsOnImport"),
                ],
            ),
            Group(
                "newBookmarks",
                &[
                    Cmd("bookmarksView.useCurrentPage"),
                    Cmd("bookmarksView.useInheritedZoom"),
                    Cmd("bookmarksView.useRegionForDest"),
                    Cmd("bookmarksView.useTextForTitle"),
                    Cmd("bookmarksView.createOptions"),
                ],
            ),
            Group(
                "display",
                &[
                    Cmd("bookmarksView.textSize"),
                    Cmd("bookmarksView.wrapTitles"),
                    Cmd("bookmarksView.showIcons"),
                    Cmd("bookmarksView.showContrastText"),
                    Cmd("bookmarksView.autoExpandBookmarks"),
                    Cmd("bookmarksView.ensureVisibility"),
                    Cmd("bookmarksView.viewOptions"),
                ],
            ),
            Sep,
            Cmd("bookmarksView.importExport"),
            Cmd("bookmarksView.options"),
            Cmd("bookmarksView.prefs"),
            More,
        ],
    ),
    (
        "layersView",
        &[
            Cmd("layersView.addNewLayer"),
            Cmd("layersView.addNewGroup"),
            Cmd("layersView.importAsLayer"),
            Cmd("layersView.extract"),
            Sep,
            Group(
                "select",
                &[
                    Cmd("layersView.select"),
                    Cmd("layersView.selectLayers"),
                    Cmd("layersView.selectVisibleLayers"),
                    Cmd("layersView.selectHiddenLayers"),
                    Cmd("layersView.selectEmptyLayers"),
                    Cmd("layersView.selectTextLabels"),
                    Cmd("layersView.selectCorresponding"),
                ],
            ),
            Group(
                "visibility",
                &[
                    Cmd("layersView.showSelected"),
                    Cmd("layersView.hideSelected"),
                    Cmd("layersView.toggleSelected"),
                    Sep,
                    Cmd("layersView.setAsDefault"),
                    Cmd("layersView.setCurrentStatesAsDefault"),
                    Cmd("layersView.applyOverrides"),
                    Cmd("layersView.resetOverrides"),
                    Cmd("layersView.overrideByParent"),
                ],
            ),
            Group(
                "mergeFlatten",
                &[
                    Cmd("layersView.merge"),
                    Cmd("layersView.mergeSelectedOnly"),
                    Cmd("layersView.mergeSelectedWithChildren"),
                    Sep,
                    Cmd("layersView.flattenSelected"),
                    Cmd("layersView.flattenOC"),
                ],
            ),
            Group(
                "display",
                &[
                    Cmd("layersView.viewAllPages"),
                    Cmd("layersView.viewVisiblePages"),
                    Cmd("layersView.sort"),
                ],
            ),
            Sep,
            Cmd("layersView.options"),
            More,
        ],
    ),
    (
        "pathedit",
        &[
            Group(
                "draw",
                &[
                    Cmd("pathedit.line"),
                    Cmd("pathedit.polyline"),
                    Cmd("pathedit.polybezier"),
                    Cmd("pathedit.freehand"),
                    Sep,
                    Cmd("pathedit.rect"),
                    Cmd("pathedit.roundRect"),
                    Cmd("pathedit.parallelogram"),
                    Cmd("pathedit.circle"),
                    Cmd("pathedit.ellipse"),
                    Cmd("pathedit.arc"),
                    Cmd("pathedit.3point"),
                ],
            ),
            Group(
                "path",
                &[
                    Cmd("pathedit.addSubPath"),
                    Cmd("pathedit.removeSubPath"),
                    Cmd("pathedit.closePath"),
                    Cmd("pathedit.join"),
                    Cmd("pathedit.split"),
                    Cmd("pathedit.reverse"),
                ],
            ),
            Group(
                "convert",
                &[
                    Cmd("pathedit.convert"),
                    Cmd("pathedit.convert2bezier"),
                    Cmd("pathedit.convert2line"),
                ],
            ),
            Cmd("pathedit.anchors"),
            Sep,
            Cmd("pathedit.finish"),
            Cmd("pathedit.cancel"),
            More,
        ],
    ),
];

/// The ids of the commands a layout places.
fn placed(items: &[Item], out: &mut Vec<&'static str>) {
    for item in items {
        match item {
            Cmd(id) => out.push(id),
            Group(_, inner) => placed(inner, out),
            Sep | More => {}
        }
    }
}

/// Separators only between entries, never two together; groups left empty go.
fn tidy(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for n in nodes {
        match n {
            Node::Sep if matches!(out.last(), None | Some(Node::Sep)) => {}
            Node::Group { nodes, .. } if nodes.is_empty() => {}
            other => out.push(other),
        }
    }
    while matches!(out.last(), Some(Node::Sep)) {
        out.pop();
    }
    out
}

/// The entries of `items`; `pool` is what the menu holds, the rest of which
/// goes into More.
fn resolve(items: &[Item], pool: &[&'static Command], all_placed: &[&str]) -> Vec<Node> {
    let reg = registry();
    let nodes = items
        .iter()
        .filter_map(|item| match item {
            Cmd(id) => reg.get(id).map(Node::Cmd),
            Sep => Some(Node::Sep),
            Group(key, inner) => Some(Node::Group {
                key,
                nodes: tidy(resolve(inner, pool, all_placed)),
            }),
            More => {
                let mut rest: Vec<&'static Command> = pool
                    .iter()
                    .copied()
                    .filter(|c| !all_placed.contains(&c.id.as_str()))
                    .collect();
                rest.sort_by(|a, b| a.label.cmp(&b.label));
                Some(Node::Group {
                    key: "more",
                    nodes: rest.into_iter().map(Node::Cmd).collect(),
                })
            }
        })
        .collect();
    tidy(nodes)
}

fn layout(items: &'static [Item], pool: &[&'static Command]) -> Vec<Node> {
    let mut ids = Vec::new();
    placed(items, &mut ids);
    resolve(items, pool, &ids)
}

/// The entries of menu `menu` of the menu bar.
pub fn menu_nodes(menu: &str) -> Vec<Node> {
    let reg = registry();
    let entries = reg.menu_entries(menu);
    match MENUS.iter().find(|(m, _)| *m == menu) {
        Some((_, items)) => {
            let pool: Vec<&'static Command> = entries.into_iter().flatten().collect();
            layout(items, &pool)
        }
        None => entries
            .into_iter()
            .map(|e| e.map(Node::Cmd).unwrap_or(Node::Sep))
            .collect(),
    }
}

/// The entries of the submenu of command `id`.
pub fn submenu_nodes(id: &str) -> Vec<Node> {
    let children = registry().children(id);
    match SUBMENUS.iter().find(|(s, _)| *s == id) {
        Some((_, items)) => layout(items, &children),
        None => children.into_iter().map(Node::Cmd).collect(),
    }
}

/// Rows a list of entries takes (separators are thin and not counted).
pub fn rows(nodes: &[Node]) -> usize {
    nodes.iter().filter(|n| !matches!(n, Node::Sep)).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Most rows at the top of a menu, in a submenu of a layout, in More.
    const TOP: usize = 22;
    const GROUP: usize = 20;
    const MORE: usize = 16;

    fn commands_in(nodes: &[Node], out: &mut Vec<String>) {
        for n in nodes {
            match n {
                Node::Cmd(c) => out.push(c.id.clone()),
                Node::Group { nodes, .. } => commands_in(nodes, out),
                Node::Sep => {}
            }
        }
    }

    fn check(name: &str, nodes: &[Node], pool: &[&Command]) {
        assert!(rows(nodes) <= TOP, "{name}: {} rows at the top", rows(nodes));
        let mut shown = Vec::new();
        commands_in(nodes, &mut shown);
        let unique: HashSet<&String> = shown.iter().collect();
        assert_eq!(unique.len(), shown.len(), "{name}: a command shown twice");
        for c in pool {
            assert!(shown.contains(&c.id), "{name}: {} is no longer reachable", c.id);
        }
        for n in nodes {
            if let Node::Group { key, nodes } = n {
                let limit = if *key == "more" { MORE } else { GROUP };
                assert!(rows(nodes) <= limit, "{name} › {key}: {} rows", rows(nodes));
            }
        }
    }

    #[test]
    fn layouts_name_only_commands_of_the_registry() {
        let reg = registry();
        for (name, items) in MENUS.iter().chain(SUBMENUS) {
            let mut ids = Vec::new();
            placed(items, &mut ids);
            for id in ids {
                assert!(reg.get(id).is_some(), "{name}: unknown command {id}");
            }
        }
        for (id, _) in SUBMENUS {
            assert!(reg.has_children(id), "{id} has no submenu");
        }
    }

    #[test]
    fn every_menu_is_short_and_keeps_its_commands() {
        let reg = registry();
        for (menu, _) in folitecta_core::commands::MENUS {
            let pool: Vec<&Command> = reg.menu_entries(menu).into_iter().flatten().collect();
            check(menu, &menu_nodes(menu), &pool);
        }
        for (id, _) in SUBMENUS {
            check(id, &submenu_nodes(id), &reg.children(id));
        }
    }

    #[test]
    fn every_submenu_of_a_layout_has_its_words() {
        fn keys(items: &[Item], out: &mut Vec<&'static str>) {
            for item in items {
                if let Group(key, inner) = item {
                    out.push(key);
                    keys(inner, out);
                }
            }
        }
        let mut all = vec!["more"];
        for (_, items) in MENUS.iter().chain(SUBMENUS) {
            keys(items, &mut all);
        }
        for key in all {
            let k = format!("menu.group.{key}");
            for lang in [crate::i18n::Lang::En, crate::i18n::Lang::It] {
                assert_ne!(crate::i18n::t(lang, &k), k, "{k} in {lang:?}");
            }
        }
    }

    #[test]
    fn separators_stand_between_entries() {
        for (menu, _) in folitecta_core::commands::MENUS {
            let nodes = menu_nodes(menu);
            assert!(!matches!(nodes.first(), Some(Node::Sep)), "{menu}");
            assert!(!matches!(nodes.last(), Some(Node::Sep)), "{menu}");
            assert!(
                !nodes.windows(2).any(|w| matches!(w, [Node::Sep, Node::Sep])),
                "{menu}"
            );
        }
    }
}
