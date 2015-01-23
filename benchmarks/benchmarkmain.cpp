#include <hayai.hpp>
#include <boost/format.hpp>

#include <humblelogging/api.h>

#include <db.h>
#include <query.h>
#include <annotationsearch.h>
#include <regexannosearch.h>
#include <operators/precedence.h>
#include <operators/inclusion.h>
#include <operators/dominance.h>
#include <operators/overlap.h>
#include <operators/pointing.h>
#include <wrapper.h>

HUMBLE_LOGGER(logger, "default");

using namespace annis;

class Tiger
    : public ::hayai::Fixture
{
public:

  virtual void SetUp()
  {
    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    dbLoaded = db.load(dataDir + "/tiger2");
    counter = 0;
  }

  /// After each run, clear the vector of random integers.
  virtual void TearDown()
  {
     HL_INFO(logger, (boost::format("result %1%") % counter).str());
  }

  DB db;
  bool dbLoaded;

  unsigned int counter;

};

class TigerFallback
    : public ::hayai::Fixture
{
public:

  TigerFallback()
    :db(false)
  {

  }

  virtual void SetUp()
  {
    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    dbLoaded = db.load(dataDir + "/tiger2");
    counter = 0;
  }

  /// After each run, clear the vector of random integers.
  virtual void TearDown()
  {
     HL_INFO(logger, (boost::format("result %1%") % counter).str());
  }

  DB db;
  bool dbLoaded;

  unsigned int counter;

};

class Ridges
    : public ::hayai::Fixture
{
public:

  virtual void SetUp()
  {
    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    dbLoaded = db.load(dataDir + "/ridges");
    counter = 0;
  }

  /// After each run, clear the vector of random integers.
  virtual void TearDown()
  {
     HL_INFO(logger, (boost::format("result %1%") % counter).str());
  }

  DB db;
  bool dbLoaded;

  unsigned int counter;

};

class RidgesFallback
    : public ::hayai::Fixture
{
public:

  RidgesFallback()
    :db(false)
  {

  }

  virtual void SetUp()
  {
    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    dbLoaded = db.load(dataDir + "/ridges");
    counter = 0;
  }

  /// After each run, clear the vector of random integers.
  virtual void TearDown()
  {
     HL_INFO(logger, (boost::format("result %1%") % counter).str());
  }

  DB db;
  bool dbLoaded;

  unsigned int counter;

};

class Tueba
    : public ::hayai::Fixture
{
public:

  Tueba()
    :db(true)
  {

  }

  virtual void SetUp()
  {
    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    dbLoaded = db.load(dataDir + "/tuebadz6");
    counter = 0;
  }

  /// After each run, clear the vector of random integers.
  virtual void TearDown()
  {
     HL_INFO(logger, (boost::format("result %1%") % counter).str());
  }

  DB db;
  bool dbLoaded;

  unsigned int counter;

};

class TuebaFallback
    : public ::hayai::Fixture
{
public:

  TuebaFallback()
    :db(false)
  {

  }

  virtual void SetUp()
  {
    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    dbLoaded = db.load(dataDir + "/tuebadz6");
    counter = 0;
  }

  /// After each run, clear the vector of random integers.
  virtual void TearDown()
  {
     HL_INFO(logger, (boost::format("result %1%") % counter).str());
  }

  DB db;
  bool dbLoaded;

  unsigned int counter;

};


BENCHMARK_F(Tiger, Cat, 5, 1)
{
  AnnotationNameSearch search(db, "cat");
  counter=0;
  while(search.hasNext())
  {
    search.next();
    counter++;
  }
}

// cat="S" & tok="Bilharziose" & #1 >* #2
BENCHMARK_F(Tiger, BilharzioseSentence, 5, 1)
{
  Query q(db);
  auto n1 = q.addNode(std::make_shared<AnnotationNameSearch>(db, "tiger", "cat", "S"));
  auto n2 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_tok, "Bilharziose"));

  q.addOperator(std::make_shared<Dominance>(db, "", "", 1, uintmax), n1, n2);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }

}

// pos="NN" .2,10 pos="ART" . pos="NN"
BENCHMARK_F(Tiger, NNPreARTPreNN, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "tiger", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "tiger", "pos", "ART"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "tiger", "pos", "NN"));

  q.addOperator(std::make_shared<Precedence>(db, 2,10), 0, 1);
  q.addOperator(std::make_shared<Precedence>(db), 1, 2);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// pos="NN" .2,10 pos="ART" . pos="NN"
BENCHMARK_F(TigerFallback, NNPreARTPreNN, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "tiger", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "tiger", "pos", "ART"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "tiger", "pos", "NN"));

  q.addOperator(std::make_shared<Precedence>(db, 2,10), 0, 1);
  q.addOperator(std::make_shared<Precedence>(db), 1, 2);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// cat=/(.P)/ >* /A.*/
BENCHMARK_F(Tiger, RegexDom, 1, 1) {

  Query q(db);
  auto n1 = q.addNode(std::make_shared<RegexAnnoSearch>(db,
                                                        "cat",".P"));
  auto n2 = q.addNode(std::make_shared<RegexAnnoSearch>(db,
                                                        annis_ns, annis_tok,
                                                       "A.*"));

  q.addOperator(std::make_shared<Dominance>(db, "", "", 1, uintmax), n1, n2);

  while(q.hasNext())
  {
    std::vector<Match> m = q.next();
    counter++;
  }
}

// pos="NN" & norm="Blumen" & #1 _i_ #2
BENCHMARK_F(Ridges, PosNNIncludesNormBlumen, 5, 1) {


  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));

  q.addOperator(std::make_shared<annis::Inclusion>(db), 1, 0);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// pos="NN" & norm="Blumen" & #1 _o_ #2
BENCHMARK_F(Ridges, PosNNOverlapsNormBlumen, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));
  q.addOperator(std::make_shared<Overlap>(db), 0, 1);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// pos="NN" .2,10 pos="ART"
BENCHMARK_F(Ridges, NNPreceedingART, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "ART"));

  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// tok .2,10 tok
BENCHMARK_F(Ridges, TokPreceedingTok, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns,annis::annis_tok));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns,annis::annis_tok));


  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// pos="NN" .2,10 pos="ART"
BENCHMARK_F(RidgesFallback, NNPreceedingART, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "ART"));

  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// tok .2,10 tok
BENCHMARK_F(RidgesFallback, TokPreceedingTok, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns, annis::annis_tok));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, annis::annis_ns,annis::annis_tok));

  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// pos="NN" & norm="Blumen" & #1 _i_ #2
BENCHMARK_F(RidgesFallback, PosNNIncludesNormBlumen, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));
  q.addOperator(std::make_shared<annis::Inclusion>(db), 1, 0);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

// pos="NN" & norm="Blumen" & #1 _o_ #2
BENCHMARK_F(RidgesFallback, PosNNOverlapsNormBlumen, 5, 1) {

  Query q(db);
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "pos", "NN"));
  q.addNode(std::make_shared<AnnotationNameSearch>(db, "default_ns", "norm", "Blumen"));
  q.addOperator(std::make_shared<Overlap>(db), 0, 1);

  while(q.hasNext())
  {
    q.next();
    counter++;
  }
}

/*
node & merged:pos="PPER" & node & mmax:relation="anaphoric" & node & node & mmax:relation="anaphoric"
& #1 >[func="ON"] #3
& #3 >* #2
& #2 _i_ #4
& #5 >[func="ON"] #6
& #6 >* #7
& #4 ->anaphoric #7
*/
BENCHMARK_F(TuebaFallback, Complex, 5, 1) {

  Query q(db);
  auto n1 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n2 = q.addNode(std::make_shared<AnnotationNameSearch>(db, "merged", "pos", "PPER"));
  auto n3 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n4 = q.addNode(std::make_shared<AnnotationNameSearch>(db, "mmax", "relation", "anaphoric"));
  auto n5 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n6 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n7 = q.addNode(std::make_shared<AnnotationNameSearch>(db, "mmax", "relation", "anaphoric"));

  Annotation funcOnAnno =
      Init::initAnnotation(db.strings.add("func"), db.strings.add("ON"));

  q.addOperator(std::make_shared<Inclusion>(db), n2, n4);
  q.addOperator(std::make_shared<Pointing>(db, "", "anaphoric"), n4, n7);
  q.addOperator(std::make_shared<Dominance>(db, "", "", funcOnAnno), n1, n3);
  q.addOperator(std::make_shared<Dominance>(db, "", "", 1, uintmax), n3, n2);
  q.addOperator(std::make_shared<Dominance>(db, "", "", funcOnAnno), n5, n6);
  q.addOperator(std::make_shared<Dominance>(db, "", "", 1, uintmax), n6, n7);

  unsigned int counter=0;
  while(q.hasNext() && counter < 10u)
  {
    q.next();
    counter++;
  }
}

/*
node & merged:pos="PPER" & node & mmax:relation="anaphoric" & node & node & mmax:relation="anaphoric"
& #1 >[func="ON"] #3
& #3 >* #2
& #2 _i_ #4
& #5 >[func="ON"] #6
& #6 >* #7
& #4 ->anaphoric #7
*/
BENCHMARK_F(Tueba, Complex, 5, 1) {

  Query q(db);
  auto n1 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n2 = q.addNode(std::make_shared<AnnotationNameSearch>(db, "merged", "pos", "PPER"));
  auto n3 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n4 = q.addNode(std::make_shared<AnnotationNameSearch>(db, "mmax", "relation", "anaphoric"));
  auto n5 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n6 = q.addNode(std::make_shared<AnnotationNameSearch>(db, annis_ns, annis_node_name));
  auto n7 = q.addNode(std::make_shared<AnnotationNameSearch>(db, "mmax", "relation", "anaphoric"));

  Annotation funcOnAnno =
      Init::initAnnotation(db.strings.add("func"), db.strings.add("ON"));

  q.addOperator(std::make_shared<Inclusion>(db), n2, n4);
  q.addOperator(std::make_shared<Pointing>(db, "", "anaphoric"), n4, n7);
  q.addOperator(std::make_shared<Dominance>(db, "", "", funcOnAnno), n1, n3);
  q.addOperator(std::make_shared<Dominance>(db, "", "", 1, uintmax), n3, n2);
  q.addOperator(std::make_shared<Dominance>(db, "", "", funcOnAnno), n5, n6);
  q.addOperator(std::make_shared<Dominance>(db, "", "", 1, uintmax), n6, n7);

  unsigned int counter=0;
  while(q.hasNext() && counter < 10u)
  {
    q.next();
    counter++;
  }
}

int main(int argc, char** argv)
{
  humble::logging::Factory &fac = humble::logging::Factory::getInstance();

  fac.setDefaultLogLevel(humble::logging::LogLevel::Info);
  fac.registerAppender(new humble::logging::FileAppender("benchmark_annis4.log"));


  hayai::ConsoleOutputter consoleOutputter;

  hayai::Benchmarker::AddOutputter(consoleOutputter);
  if(argc >= 2)
  {
    for(int i=1; i < argc; i++)
    {
      std::cout << "adding include filter" << argv[i] << std::endl;
      hayai::Benchmarker::AddIncludeFilter(argv[i]);
    }
  }
  hayai::Benchmarker::RunAllTests();
  return 0;
}
