// Explicit symbolic GC roots. Implements [SYMEX-GC-ROOTS].
#include "llvm/Analysis/ValueTracking.h"
#include "llvm/ADT/DenseMap.h"
#include "llvm/ADT/SmallPtrSet.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/DerivedTypes.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/GlobalVariable.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/PassManager.h"
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Passes/PassPlugin.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/Transforms/Utils/ModuleUtils.h"

using namespace llvm;

namespace {

constexpr const char *kFrameHeadName = "osp_gc_root_frames";
constexpr const char *kTableHeadName = "osp_gc_global_root_tables";
constexpr const char *kWordName = "osp.gc.slot";
constexpr const char *kRangesName = "osp.gc.ranges";
constexpr const char *kFrameName = "osp.gc.frame";
constexpr const char *kCtorName = "osp.gc.roots.register";
constexpr const char *kRangeName = "osp.gc.global.range";
constexpr const char *kTableName = "osp.gc.global.table";

struct ModuleTypes {
  LLVMContext &Ctx;
  Type *I64;
  PointerType *Ptr;
  StructType *Range;
  StructType *Frame;
  StructType *Table;

  explicit ModuleTypes(LLVMContext &C) : Ctx(C) {
    I64 = Type::getInt64Ty(Ctx);
    Ptr = PointerType::get(Ctx, 0);
    Range = StructType::get(Ctx, {I64, I64});
    Frame = StructType::get(Ctx, {Ptr, I64, Ptr});
    Table = StructType::get(Ctx, {Ptr, I64, Ptr});
  }
};

// --- IR primitives ---------------------------------------------------------

bool fail(LLVMContext &Ctx, const Twine &Msg) {
  Ctx.emitError(Twine("osprey-gc-roots: ") + Msg);
  return false;
}

bool isRootWordType(Type *Ty) {
  return Ty->isSized() && !Ty->isVoidTy() && !Ty->isTokenTy() && !Ty->isLabelTy() && !Ty->isMetadataTy();
}

StoreInst *volatileStore(IRBuilder<> &B, Value *V, Value *Addr) {
  StoreInst *S = B.CreateStore(V, Addr);
  S->setVolatile(true);
  return S;
}

LoadInst *volatileLoad(IRBuilder<> &B, Type *Ty, Value *Addr, const Twine &N) {
  LoadInst *L = B.CreateLoad(Ty, Addr, N);
  L->setVolatile(true);
  return L;
}

Constant *fixedBytes(const ModuleTypes &T, const DataLayout &DL, Type *Ty) {
  return ConstantInt::get(T.I64, DL.getTypeAllocSize(Ty).getFixedValue());
}

Value *rangePtr(IRBuilder<> &B, const ModuleTypes &T, Value *Ranges, unsigned Index) {
  return B.CreateInBoundsGEP(T.Range, Ranges, ConstantInt::get(T.I64, Index));
}

void storeRange(IRBuilder<> &B, const ModuleTypes &T, Value *Ranges, unsigned Index, Value *Address, Value *Bytes) {
  Value *R = rangePtr(B, T, Ranges, Index);
  volatileStore(B, Address, B.CreateStructGEP(T.Range, R, 0));
  volatileStore(B, Bytes, B.CreateStructGEP(T.Range, R, 1));
}

GlobalVariable *getOrCreateHeadGlobal(Module &M, ModuleTypes &T, StringRef Name) {
  GlobalVariable *GV = M.getGlobalVariable(Name);
  if (!GV)
    return new GlobalVariable(M, T.Ptr, /*isConstant=*/false, GlobalValue::ExternalLinkage, nullptr, Name);
  if (GV->getValueType() != T.Ptr) {
    M.getContext().emitError("osprey-gc-roots: global '" + Name + "' is not an opaque pointer");
    return nullptr;
  }
  return GV;
}

// --- Collector exclusion ---------------------------------------------------

// The collector's own frames must never be instrumented: its stack layout and
// its internal scanning code are the trusted base of the runtime.
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

// --- Function instrumentation plan (data only) -----------------------------

struct SlotPlan {
  DenseMap<Value *, unsigned> index;
  SmallVector<Value *, 64> values;
};

struct AllocaPlan {
  DenseMap<AllocaInst *, unsigned> index;
  SmallVector<AllocaInst *, 16> items;
};

struct FrameLayout {
  AllocaInst *frame = nullptr;
  AllocaInst *ranges = nullptr;
  SmallVector<AllocaInst *, 64> slots;
  unsigned numRanges = 0;
  Value *previous = nullptr;
};

struct FunctionPlan {
  Function &F;
  ModuleTypes &T;
  const DataLayout &DL;
  GlobalVariable *head;
  SmallVector<Instruction *, 128> original;
  SlotPlan slots;
  AllocaPlan allocas;
  FrameLayout layout;
};

// --- Body inventory and validation ----------------------------------------

bool isUnsupportedControl(const Instruction &I) {
  if (isa<InvokeInst>(I) || isa<CallBrInst>(I)) return true;
  if (const auto *Call = dyn_cast<CallInst>(&I)) return Call->isMustTailCall();
  const auto *AI = dyn_cast<AllocaInst>(&I);
  return AI && !AI->isStaticAlloca();
}

bool isStackRestore(const Instruction &I) {
  const auto *Call = dyn_cast<CallBase>(&I);
  const auto *Target = Call ? Call->getCalledFunction() : nullptr;
  return Target && Target->getName().starts_with("llvm.stackrestore");
}

// Reject every construct whose root lifetime this pass cannot represent.
bool validateBody(const Function &F) {
  for (const BasicBlock &BB : F)
    for (const Instruction &I : BB) {
      if (isUnsupportedControl(I))
        return fail(F.getContext(), "unsupported stack control in " + F.getName());
      if (isStackRestore(I))
        return fail(F.getContext(), "stackrestore requires root lifetime tracking");
    }
  return true;
}

// Snapshot the original instructions before any instrumentation is added.
SmallVector<Instruction *, 128> snapshotBody(const Function &F) {
  SmallVector<Instruction *, 128> Original;
  for (const BasicBlock &BB : F)
    for (const Instruction &I : BB) Original.push_back(const_cast<Instruction *>(&I));
  return Original;
}

void noteSlot(SlotPlan &P, Value *V) {
  if (!isRootWordType(V->getType()) || P.index.count(V)) return;
  P.index[V] = P.values.size();
  P.values.push_back(V);
}

SlotPlan planSlots(Function &F, ArrayRef<Instruction *> Original) {
  SlotPlan P;
  for (Argument &A : F.args())
    noteSlot(P, &A);
  for (Instruction *I : Original)
    if (!I->getType()->isVoidTy()) noteSlot(P, I);
  return P;
}

bool validateAlloca(const AllocaInst &AI, const Function &F, const DataLayout &DL) {
  if (DL.getTypeAllocSize(AI.getAllocatedType()).isScalable())
    return fail(F.getContext(), "scalable alloca type in function '" + F.getName() + "' has no fixed byte size");
  if (AI.getArraySize() && AI.getArraySize()->getType()->isVectorTy())
    return fail(F.getContext(), "vector alloca count in function '" + F.getName() + "' is unsupported");
  return true;
}

bool planAllocas(Function &F, const DataLayout &DL, unsigned SlotCount, ArrayRef<Instruction *> Original, AllocaPlan &Out) {
  for (Instruction *I : Original) {
    auto *AI = dyn_cast<AllocaInst>(I);
    if (!AI) continue;
    if (!validateAlloca(*AI, F, DL)) return false;
    Out.index[AI] = SlotCount + Out.items.size();
    Out.items.push_back(AI);
  }
  return true;
}

// --- Frame construction ----------------------------------------------------

FrameLayout allocateLayout(IRBuilder<> &B, const ModuleTypes &T, const SlotPlan &S, unsigned NumRanges) {
  FrameLayout L;
  L.numRanges = NumRanges;
  L.frame = B.CreateAlloca(T.Frame, nullptr, kFrameName);
  L.ranges = NumRanges ? B.CreateAlloca(T.Range, ConstantInt::get(T.I64, NumRanges), kRangesName) : nullptr;
  for (Value *V : S.values) L.slots.push_back(B.CreateAlloca(V->getType(), nullptr, kWordName));
  return L;
}

// A frame can be scanned before an alloca in a non-entry block (or a
// not-yet-executed definition) runs, so every range and word starts zeroed.
void zeroLayout(IRBuilder<> &B, const ModuleTypes &T, const FrameLayout &L) {
  for (unsigned I = 0; I < L.numRanges; ++I)
    volatileStore(B, ConstantAggregateZero::get(T.Range), rangePtr(B, T, L.ranges, I));
  for (AllocaInst *Slot : L.slots)
    volatileStore(B, Constant::getNullValue(Slot->getAllocatedType()), Slot);
}

void storeSlotRanges(IRBuilder<> &B, const FunctionPlan &P) {
  for (unsigned I = 0; I < P.layout.slots.size(); ++I)
    storeRange(B, P.T, P.layout.ranges, I, B.CreatePtrToInt(P.layout.slots[I], P.T.I64),
               fixedBytes(P.T, P.DL, P.slots.values[I]->getType()));
}

void spill(IRBuilder<> &B, Value *V, const FunctionPlan &P) {
  auto It = P.slots.index.find(V);
  if (It != P.slots.index.end()) volatileStore(B, V, P.layout.slots[It->second]);
}

// previous first, then the head, so a collector never observes a half-built frame.
void pushFrameValue(IRBuilder<> &B, FunctionPlan &P) {
  Value *Prev = volatileLoad(B, P.T.Ptr, P.head, "osp.gc.prev");
  Value *Ranges = P.layout.ranges ? static_cast<Value *>(P.layout.ranges) : ConstantPointerNull::get(P.T.Ptr);
  volatileStore(B, Prev, B.CreateStructGEP(P.T.Frame, P.layout.frame, 0));
  volatileStore(B, ConstantInt::get(P.T.I64, P.layout.numRanges), B.CreateStructGEP(P.T.Frame, P.layout.frame, 1));
  volatileStore(B, Ranges, B.CreateStructGEP(P.T.Frame, P.layout.frame, 2));
  volatileStore(B, P.layout.frame, P.head);
  P.layout.previous = Prev;
}

void emitPrologue(FunctionPlan &P) {
  BasicBlock &Entry = P.F.getEntryBlock();
  IRBuilder<> B(&Entry, Entry.getFirstInsertionPt());
  P.layout = allocateLayout(B, P.T, P.slots, P.slots.values.size() + P.allocas.items.size());
  zeroLayout(B, P.T, P.layout);
  storeSlotRanges(B, P);
  for (Argument &A : P.F.args()) spill(B, &A, P);
  pushFrameValue(B, P);
}

// --- Definition spills -----------------------------------------------------

void publishAllocaRange(IRBuilder<> &B, AllocaInst *AI, const FunctionPlan &P) {
  Value *Bytes = fixedBytes(P.T, P.DL, AI->getAllocatedType());
  if (Value *Count = AI->getArraySize()) Bytes = B.CreateMul(Bytes, B.CreateZExtOrTrunc(Count, P.T.I64));
  storeRange(B, P.T, P.layout.ranges, P.allocas.index.lookup(AI), B.CreatePtrToInt(AI, P.T.I64), Bytes);
}

PHINode *lastPhi(BasicBlock &BB) {
  PHINode *Last = nullptr;
  for (Instruction &X : BB) {
    auto *Phi = dyn_cast<PHINode>(&X);
    if (!Phi) break;
    Last = Phi;
  }
  return Last;
}

void spillPhiGroup(BasicBlock &BB, const FunctionPlan &P) {
  IRBuilder<> B(lastPhi(BB)->getNextNode());
  for (Instruction &X : BB) {
    auto *Phi = dyn_cast<PHINode>(&X);
    if (!Phi) break;
    spill(B, Phi, P);
  }
}

void spillDefinitions(ArrayRef<Instruction *> Original, const FunctionPlan &P) {
  SmallPtrSet<BasicBlock *, 16> Done;
  for (Instruction *I : Original) {
    if (auto *Call = dyn_cast<CallBase>(I); Call && !Call->isInlineAsm()) {
      Call->removeFnAttr(Attribute::Memory);
      Call->removeFnAttr(Attribute::Speculatable);
    }
    if (auto *Phi = dyn_cast<PHINode>(I)) {
      if (Done.insert(Phi->getParent()).second) spillPhiGroup(*Phi->getParent(), P);
      continue;
    }
    Instruction *Next = I->getNextNode();
    if (!Next || (I->getType()->isVoidTy() && !isa<AllocaInst>(I))) continue; // a terminator never defines a word
    IRBuilder<> B(Next);
    spill(B, I, P);
    if (auto *AI = dyn_cast<AllocaInst>(I)) publishAllocaRange(B, AI, P);
  }
}

// --- Frame teardown --------------------------------------------------------

// A musttail call must stay adjacent to its ret, so the pop goes before the call.
Instruction *popAnchor(ReturnInst *Ret) {
  Instruction *Previous = Ret->getPrevNode();
  const auto *Call = Previous ? dyn_cast<CallInst>(Previous) : nullptr;
  return Call && Call->isMustTailCall() ? Previous : Ret;
}

void emitReturns(Function &F, GlobalVariable *Head, Value *Previous) {
  for (BasicBlock &BB : F)
    if (auto *Ret = dyn_cast<ReturnInst>(BB.getTerminator())) {
      IRBuilder<> B(popAnchor(Ret));
      volatileStore(B, Previous, Head);
    }
}

bool instrumentFunction(Function &F, ModuleTypes &T, const DataLayout &DL, GlobalVariable *Head) {
  if (!validateBody(F)) return false;
  F.removeFnAttr(Attribute::Memory);
  F.removeFnAttr(Attribute::Speculatable);
  FunctionPlan P{F, T, DL, Head, snapshotBody(F), SlotPlan(), AllocaPlan(), FrameLayout()};
  P.slots = planSlots(F, P.original);
  if (!planAllocas(F, P.DL, P.slots.values.size(), P.original, P.allocas)) return false;
  emitPrologue(P);
  spillDefinitions(P.original, P);
  emitReturns(F, Head, P.layout.previous);
  return true;
}

// --- Module globals --------------------------------------------------------

// Register every module global definition in its own private Table, pushed by
// one priority-0 constructor. Heap tables are intentionally not walked here:
// only global values are collected explicitly.
bool validateGlobal(const GlobalVariable &GV, ModuleTypes &T, const DataLayout &DL) {
  return !DL.getTypeAllocSize(GV.getValueType()).isScalable() ||
         fail(T.Ctx, "scalable global '" + GV.getName() + "' has no fixed byte size");
}

GlobalVariable *emitRangeArray(Module &M, const ModuleTypes &T, const DataLayout &DL, GlobalVariable *GV) {
  Constant *Element = ConstantStruct::get(T.Range, {ConstantExpr::getPtrToInt(GV, T.I64), fixedBytes(T, DL, GV->getValueType())});
  ArrayType *Ty = ArrayType::get(T.Range, 1);
  auto *RangeGV = new GlobalVariable(M, Ty, /*isConstant=*/true, GlobalValue::PrivateLinkage, ConstantArray::get(Ty, {Element}), kRangeName);
  RangeGV->setUnnamedAddr(GlobalValue::UnnamedAddr::Global);
  return RangeGV;
}

GlobalVariable *emitTable(Module &M, const ModuleTypes &T, Constant *RangeGV) {
  Constant *Init = ConstantStruct::get(T.Table, {ConstantPointerNull::get(T.Ptr), ConstantInt::get(T.I64, 1), RangeGV});
  auto *TableGV = new GlobalVariable(M, T.Table, /*isConstant=*/false, GlobalValue::PrivateLinkage, Init, kTableName);
  TableGV->setUnnamedAddr(GlobalValue::UnnamedAddr::Global);
  return TableGV;
}

void emitGlobalCtor(Module &M, const ModuleTypes &T, ArrayRef<GlobalVariable *> Tables, GlobalVariable *Head) {
  Function *Ctor = Function::Create(FunctionType::get(Type::getVoidTy(T.Ctx), {}, false), GlobalValue::InternalLinkage, kCtorName, M);
  Ctor->setDSOLocal(true);
  IRBuilder<> B(BasicBlock::Create(T.Ctx, "entry", Ctor));
  for (GlobalVariable *TableGV : Tables) {
    Value *Prev = volatileLoad(B, T.Ptr, Head, "osp.gc.table.head");
    volatileStore(B, Prev, B.CreateStructGEP(T.Table, TableGV, 0));
    volatileStore(B, TableGV, Head);
  }
  B.CreateRetVoid();
  appendToGlobalCtors(M, Ctor, /*Priority=*/0);
}

bool registerGlobals(Module &M, ModuleTypes &T, ArrayRef<GlobalVariable *> Globals, GlobalVariable *TableHead) {
  SmallVector<GlobalVariable *, 16> Tables;
  for (GlobalVariable *GV : Globals) {
    const DataLayout &DL = M.getDataLayout();
    if (!validateGlobal(*GV, T, DL)) return false;
    GlobalVariable *Table = emitTable(M, T, emitRangeArray(M, T, DL, GV));
    Tables.push_back(Table);
  }
  if (!Tables.empty()) emitGlobalCtor(M, T, Tables, TableHead);
  return true;
}

// --- Pass entry ------------------------------------------------------------

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

// Snapshot both inventories before the pass adds instrumentation globals and
// its constructor.
struct ModulePlan {
  ModuleTypes T;
  GlobalVariable *frameHead;
  GlobalVariable *tableHead;
  SmallVector<Function *, 64> functions;
  SmallVector<GlobalVariable *, 32> globals;
  unsigned instrumented = 0;
  unsigned collectors = 0;

  explicit ModulePlan(Module &M)
      : T(M.getContext()), frameHead(getOrCreateHeadGlobal(M, T, kFrameHeadName)),
        tableHead(getOrCreateHeadGlobal(M, T, kTableHeadName)), functions(definedFunctions(M)),
        globals(definingGlobals(M)) {}
};

bool instrumentModule(Module &M, ModulePlan &P) {
  const DataLayout &DL = M.getDataLayout();
  for (Function *F : P.functions) {
    if (isCollectorImplementation(*F)) {
      ++P.collectors;
      continue;
    }
    if (F->hasFnAttribute(Attribute::Naked)) return fail(M.getContext(), "naked function has no root lifetime");
    if (!instrumentFunction(*F, P.T, DL, P.frameHead)) return false;
    ++P.instrumented;
  }
  return true;
}

void report(Module &M, const ModulePlan &P) {
  errs() << "[osprey-gc-roots] " << M.getModuleIdentifier() << ": instrumented " << P.instrumented
         << " function(s), skipped " << P.collectors << " collector function(s), registered "
         << P.globals.size() << " global(s)\n";
}

struct OspreyGcRootsPass : PassInfoMixin<OspreyGcRootsPass> {
  PreservedAnalyses run(Module &M, ModuleAnalysisManager &) {
    if (M.getModuleFlag("osprey.gc.roots")) return PreservedAnalyses::all();
    if (M.getDataLayout().getPointerSizeInBits() != 64) {
      fail(M.getContext(), "only 64-bit symbolic modules are supported");
      return PreservedAnalyses::all();
    }
    ModulePlan P(M);
    if (!P.frameHead || !P.tableHead) return PreservedAnalyses::all();
    if (!instrumentModule(M, P) || !registerGlobals(M, P.T, P.globals, P.tableHead))
      return PreservedAnalyses::all();
    M.addModuleFlag(Module::Error, "osprey.gc.roots", 1);
    report(M, P);
    return PreservedAnalyses::none();
  }
};

} // namespace

llvm::PassPluginLibraryInfo getOspreyGcRootsPluginInfo() {
  auto Register = [](PassBuilder &PB) {
    PB.registerPipelineParsingCallback(
        [](StringRef Name, ModulePassManager &MPM, ArrayRef<PassBuilder::PipelineElement>) {
          if (Name != "osprey-gc-roots") return false;
          MPM.addPass(OspreyGcRootsPass());
          return true;
        });
  };
  return {LLVM_PLUGIN_API_VERSION, "osprey-gc-roots", "0.1", Register};
}

extern "C" LLVM_ATTRIBUTE_WEAK ::llvm::PassPluginLibraryInfo
llvmGetPassPluginInfo() {
  return getOspreyGcRootsPluginInfo();
}
