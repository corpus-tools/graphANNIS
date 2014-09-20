#ifndef SEARCHTESTPCC2_H
#define SEARCHTESTPCC2_H

#include "gtest/gtest.h"
#include "db.h"
#include "annotationsearch.h"

#include <vector>

using namespace annis;

class SearchTestPcc2 : public ::testing::Test {
 protected:
  DB db;
  SearchTestPcc2()
  {
  }

  virtual ~SearchTestPcc2() {
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
    bool loadedDB = db.load(dataDir + "/pcc2");
    EXPECT_EQ(true, loadedDB);

  }

  virtual void TearDown() {
    // Code here will be called immediately after each test (right
    // before the destructor).
  }

  // Objects declared here can be used by all tests in the test case for Foo.
};

TEST_F(SearchTestPcc2, CatSearch) {
  AnnotationNameSearch search(db, "cat");
  unsigned int counter=0;
  while(search.hasNext())
  {
    Match m = search.next();
    ASSERT_STREQ("cat", db.strings.str(m.second.name).c_str());
    ASSERT_STREQ("tiger", db.strings.str(m.second.ns).c_str());
    counter++;
  }

  EXPECT_EQ(155, counter);
}

TEST_F(SearchTestPcc2, TokenIndex) {
  AnnotationNameSearch n1(db, annis_ns, "tok", "Die");

  unsigned int counter=0;

  Component c = initComponent(ComponentType::ORDERING, annis_ns, "");
  const EdgeDB* edb = db.getEdgeDB(c);
  if(edb != NULL)
  {
    while(n1.hasNext())
    {
      AnnotationNameSearch n2(db, annis_ns, "tok", "Jugendlichen");

      Match m1 = n1.next();
      while(n2.hasNext())
      {
        Match m2 = n2.next();

        if(edb->isConnected(initEdge(m1.first, m2.first)))
        {
          counter++;
        }
      }
    }
  }

  EXPECT_EQ(2, counter);
}

TEST_F(SearchTestPcc2, IsConnectedRange) {
  AnnotationNameSearch n1(db, annis_ns, "tok", "Jugendlichen");

  unsigned int counter=0;

  Component c = initComponent(ComponentType::ORDERING, annis_ns, "");
  const EdgeDB* edb = db.getEdgeDB(c);
  if(edb != NULL)
  {
    while(n1.hasNext())
    {
      AnnotationNameSearch n2(db, annis_ns, "tok", "Musikcafé");

      Match m1 = n1.next();
      while(n2.hasNext())
      {
        Match m2 = n2.next();

        if(edb->isConnected(initEdge(m1.first, m2.first), 3, 10))
        {
          counter++;
        }
      }
    }
  }
  EXPECT_EQ(1, counter);
}

TEST_F(SearchTestPcc2, DepthFirst) {
    AnnotationNameSearch n1(db, annis_ns, "tok", "Tiefe");

    unsigned int counter=0;

    Component c = initComponent(ComponentType::ORDERING, annis_ns, "");
    const EdgeDB* edb = db.getEdgeDB(c);
    if(edb != NULL)
    {
      ASSERT_TRUE(n1.hasNext());
      Match m1 = n1.next();

      EdgeIterator* it = edb->findConnected(m1.first, 2, 10);
      for(std::pair<bool, std::uint32_t> connectedNode = it->next();
          connectedNode.first; connectedNode = it->next())
      {
        counter++;
      }
      delete it;
    }

  EXPECT_EQ(9, counter);
}



#endif // SEARCHTESTPCC2_H
