// Conservative call graph for [SYMEX-GC-ROOTS-NONCOLLECTING].
#ifndef OSP_GC_CALL_EFFECTS_H
#define OSP_GC_CALL_EFFECTS_H

namespace {
using namespace llvm;

bool isCollectorImplementation(const Function &F) {
  const auto *Annotations = F.getParent()->getNamedGlobal("llvm.global.annotations");
  const auto *Entries = Annotations && Annotations->hasInitializer()
      ? dyn_cast<ConstantArray>(Annotations->getInitializer()) : nullptr;
  if (!Entries) return false;
  for (const Use &Entry : Entries->operands()) {
    const auto *Fields = dyn_cast<ConstantStruct>(Entry.get());
    if (!Fields || Fields->getOperand(0)->stripPointerCasts() != &F) continue;
    StringRef Tag;
    if (getConstantStringInfo(Fields->getOperand(1), Tag) && Tag == "osprey.gc.collector") return true;
  }
  return false;
}

SmallVector<Function *, 64> definedFunctions(Module &M) {
  SmallVector<Function *, 64> Functions;
  for (Function &F : M)
    if (!F.isDeclaration() && !F.isIntrinsic()) Functions.push_back(&F);
  return Functions;
}

SmallVector<GlobalVariable *, 32> definingGlobals(Module &M) {
  SmallVector<GlobalVariable *, 32> Globals;
  for (GlobalVariable &GV : M.globals())
    if (!GV.isDeclarationForLinker() && !GV.isThreadLocal() && !GV.isConstant() && !GV.getName().starts_with("llvm.")) Globals.push_back(&GV);
  return Globals;
}

bool callsOnlyNonCollecting(const Function &F, const SmallPtrSetImpl<const Function *> &Safe) {
  for (const BasicBlock &BB : F)
    for (const Instruction &I : BB)
      if (const auto *Call = dyn_cast<CallBase>(&I)) {
        const Function *Target = Call->getCalledFunction();
        if (!Target || !Target->hasLocalLinkage() || !Safe.contains(Target)) return false;
      }
  return true;
}

// Collection is call-mediated. Unknown, indirect, intrinsic and replaceable
// calls and collector entry points remain collecting; recursive cycles keep roots.
SmallPtrSet<const Function *, 32> nonCollectingFunctions(ArrayRef<Function *> Functions) {
  SmallPtrSet<const Function *, 32> Safe;
  bool Changed;
  do {
    Changed = false;
    for (Function *F : Functions)
      if (!Safe.contains(F) && !isCollectorImplementation(*F) && callsOnlyNonCollecting(*F, Safe)) {
        Safe.insert(F);
        Changed = true;
      }
  } while (Changed);
  return Safe;
}

} // namespace
#endif
