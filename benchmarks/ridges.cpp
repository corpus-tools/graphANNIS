#include "benchmark.h"

char ridgesCorpus[] = "ridges";

class RidgesFixture : public CorpusFixture<true, ridgesCorpus>
{
public:
  virtual ~RidgesFixture() {}
};
class RidgesFallbackFixture : public CorpusFixture<false, ridgesCorpus>
{
public:
  virtual ~RidgesFallbackFixture() {}
};


// pos="NN" & norm="Blumen" & #1 _i_ #2
BASELINE_F(Ridges_PosNNIncludesNormBlumen, Fallback, RidgesFallbackFixture, 5, 1)
{

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));

  q.addOperator(std::make_shared<annis::Inclusion>(db), 1, 0);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 152u);
}
BENCHMARK_F(Ridges_PosNNIncludesNormBlumen, Optimized, RidgesFixture, 5, 1)
{

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));

  q.addOperator(std::make_shared<annis::Inclusion>(db), 1, 0);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 152u);
}

// pos="NN" & norm="Blumen" & #1 _o_ #2
BASELINE_F(Ridges_PosNNOverlapsNormBlumen, Fallback, RidgesFallbackFixture, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));
  q.addOperator(std::make_shared<Overlap>(db), 0, 1);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 152u);
}

BENCHMARK_F(Ridges_PosNNOverlapsNormBlumen, Optimized, RidgesFixture, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));
  q.addOperator(std::make_shared<Overlap>(db), 0, 1);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 152u);
}

// pos="NN" .2,10 pos="ART"
BASELINE_F(Ridges_NNPreceedingART, Fallback, RidgesFallbackFixture, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "ART"));

  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 21911u);
}
BENCHMARK_F(Ridges_NNPreceedingART, Optimized, RidgesFixture, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "ART"));

  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 21911u);
}

// tok .2,10 tok
BASELINE_F(Ridges_TokPreceedingTok, Fallback, RidgesFallbackFixture, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns,annis::annis_tok));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns,annis::annis_tok));


  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 1386828u);
}
BENCHMARK_F(Ridges_TokPreceedingTok, Optimized, RidgesFixture, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns,annis::annis_tok));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns,annis::annis_tok));


  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
  assert(counter == 1386828u);
}

