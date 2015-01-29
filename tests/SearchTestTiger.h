#ifndef SEARCHTESTTIGER_H
#define SEARCHTESTTIGER_H

#include "gtest/gtest.h"
#include "db.h"
#include "helper.h"
#include "query.h"
#include "operators/precedence.h"
#include "operators/dominance.h"
#include "exactannosearch.h"
#include "exactannokeysearch.h"
#include "wrapper.h"

#include <vector>

#include <humblelogging/api.h>

using namespace annis;

class SearchTestTiger : public ::testing::Test {
public:
  const unsigned int MAX_COUNT = 5000000u;

 protected:
  DB db;
  bool loaded;
  SearchTestTiger() {

  }

  virtual ~SearchTestTiger() {
    // You can do clean-up work that doesn't throw exceptions here.
  }

  // If the constructor and destructor are not enough for setting up
  // and cleaning up each test, you can define the following methods:

  virtual void SetUp() {

    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    bool loadedDB = db.load(dataDir + "/tiger2");
    EXPECT_EQ(true, loadedDB);
  }

  virtual void TearDown() {
    // Code here will be called immediately after each test (right
    // before the destructor).
  }

  // Objects declared here can be used by all tests in the test case for Foo.
};

TEST_F(SearchTestTiger, CatSearch) {

  ExactAnnoKeySearch search(db, "cat");
  unsigned int counter=0;
  while(search.hasNext() && counter < MAX_COUNT)
  {
    Match m = search.next();
    ASSERT_STREQ("cat", db.strings.str(m.anno.name).c_str());
    ASSERT_STREQ("tiger", db.strings.str(m.anno.ns).c_str());
    counter++;
  }

  EXPECT_EQ(373436u, counter);
}

// Should test query
// pos="NN" .2,10 pos="ART"
TEST_F(SearchTestTiger, TokenPrecedence) {

  unsigned int counter=0;

  Query q(db);
  q.addNode(std::make_shared<ExactAnnoSearch>(db, "tiger", "pos", "NN"));
  q.addNode(std::make_shared<ExactAnnoSearch>(db, "tiger", "pos", "ART"));

  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  while(q.hasNext() && counter < MAX_COUNT)
  {
    q.next();
    counter++;
  }

  EXPECT_EQ(179024u, counter);
}

// Should test query
// pos="NN" .2,10 pos="ART" . pos="NN"
TEST_F(SearchTestTiger, TokenPrecedenceThreeNodes) {

  unsigned int counter=0;

  Query q(db);
  q.addNode(std::make_shared<ExactAnnoSearch>(db, "tiger", "pos", "NN"));
  q.addNode(std::make_shared<ExactAnnoSearch>(db, "tiger", "pos", "ART"));
  q.addNode(std::make_shared<ExactAnnoSearch>(db, "tiger", "pos", "NN"));

  q.addOperator(std::make_shared<Precedence>(db, 2, 10), 0, 1);
  q.addOperator(std::make_shared<Precedence>(db), 1, 2);

  while(q.hasNext() && counter < MAX_COUNT)
  {
    q.next();
    counter++;
  }

  EXPECT_EQ(114042u, counter);
}

// cat="S" & tok="Bilharziose" & #1 >* #2
TEST_F(SearchTestTiger, BilharzioseSentence)
{
  unsigned int counter=0;

  Query q(db);

  auto n1 = q.addNode(std::make_shared<ExactAnnoSearch>(db, "tiger", "cat", "S"));
  auto n2 = q.addNode(std::make_shared<ExactAnnoSearch>(db, annis_ns, annis_tok, "Bilharziose"));

  q.addOperator(std::make_shared<Dominance>(db, "", "", 1, uintmax), n1, n2);

  while(q.hasNext())
  {
    std::vector<Match> m = q.next();
     HL_INFO(logger, (boost::format("Match %1%\t%2%\t%3%")
                      % counter
                      % db.getNodeDebugName(m[0].node)
                      % db.getNodeDebugName(m[1].node)).str()) ;
    counter++;
  }

  EXPECT_EQ(21u, counter);
}



#endif // SEARCHTESTTIGER_H
