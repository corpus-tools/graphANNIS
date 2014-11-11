#ifndef SEARCHTESTPCC2_H
#define SEARCHTESTPCC2_H

#include "gtest/gtest.h"
#include "db.h"
#include "annotationsearch.h"
#include "operators/defaultjoins.h"
#include "operators/overlap.h"
#include "operators/inclusion.h"

#include <vector>
#include <boost/format.hpp>

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
//    bool loadedDB = db.loadRelANNIS(dataDir + "/pcc2_v6_relANNIS");
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
    ASSERT_STREQ("cat", db.strings.str(m.anno.name).c_str());
    ASSERT_STREQ("tiger", db.strings.str(m.anno.ns).c_str());
    counter++;
  }

  EXPECT_EQ(155, counter);
}

TEST_F(SearchTestPcc2, MMaxAnnos) {

  AnnotationNameSearch n1(db, "mmax", "ambiguity", "not_ambig");
  AnnotationNameSearch n2(db, "mmax", "complex_np", "yes");

  unsigned int counter=0;
  while(n1.hasNext())
  {
    Match m = n1.next();
    ASSERT_STREQ("mmax", db.strings.str(m.anno.ns).c_str());
    ASSERT_STREQ("ambiguity", db.strings.str(m.anno.name).c_str());
    ASSERT_STREQ("not_ambig", db.strings.str(m.anno.val).c_str());
    counter++;
  }

  EXPECT_EQ(73, counter);

  counter=0;
  while(n2.hasNext())
  {
    Match m = n2.next();
    ASSERT_STREQ("mmax", db.strings.str(m.anno.ns).c_str());
    ASSERT_STREQ("complex_np", db.strings.str(m.anno.name).c_str());
    ASSERT_STREQ("yes", db.strings.str(m.anno.val).c_str());
    counter++;
  }

  EXPECT_EQ(17, counter);
}

TEST_F(SearchTestPcc2, TokenIndex) {
  AnnotationNameSearch n1(db, annis_ns, annis_tok, "Die");
  AnnotationNameSearch n2(db, annis_ns, annis_tok, "Jugendlichen");

  unsigned int counter=0;

  Component c = initComponent(ComponentType::ORDERING, annis_ns, "");
  const EdgeDB* edb = db.getEdgeDB(c);
  if(edb != NULL)
  {
    NestedLoopJoin join(edb, n1, n2);
    for(BinaryMatch match = join.next(); match.found; match = join.next())
    {
      counter++;
    }
  }

  EXPECT_EQ(2, counter);
}

TEST_F(SearchTestPcc2, IsConnectedRange) {
  AnnotationNameSearch n1(db, annis_ns, annis_tok, "Jugendlichen");
  AnnotationNameSearch n2(db, annis_ns, annis_tok, "Musikcafé");

  unsigned int counter=0;

  NestedLoopJoin join(db.getEdgeDB(ComponentType::ORDERING, annis_ns, ""), n1, n2, 3, 10);
  for(BinaryMatch match = join.next(); match.found; match = join.next())
  {
    counter++;
  }

  EXPECT_EQ(1, counter);
}

TEST_F(SearchTestPcc2, DepthFirst) {
    AnnotationNameSearch n1(db, annis_ns, annis_tok, "Tiefe");
    Annotation anno2 = initAnnotation(db.strings.add("node_name"), 0, db.strings.add(annis_ns));

    unsigned int counter=0;

    Component c = initComponent(ComponentType::ORDERING, annis_ns, "");
    const EdgeDB* edb = db.getEdgeDB(c);
    if(edb != NULL)
    {
      SeedJoin join(db, edb, n1, anno2, 2, 10);
      for(BinaryMatch match=join.next(); match.found; match = join.next())
      {
        counter++;
      }
    }

  EXPECT_EQ(9, counter);
}

// exmaralda:Inf-Stat="new" _o_ exmaralda:PP
TEST_F(SearchTestPcc2, TestQueryOverlap1) {
  AnnotationNameSearch n1(db, "exmaralda", "Inf-Stat", "new");
  AnnotationNameSearch n2(db, "exmaralda", "PP");

  Overlap join(db, n1, n2);

  unsigned int counter=0;
  for(BinaryMatch m=join.next(); m.found; m=join.next())
  {
    counter++;
  }

  EXPECT_EQ(3, counter);
}

// mmax:ambiguity="not_ambig" _o_ mmax:complex_np="yes"
TEST_F(SearchTestPcc2, DISABLED_TestQueryOverlap2) {
  AnnotationNameSearch n1(db, "mmax", "ambiguity", "not_ambig");
  AnnotationNameSearch n2(db, "mmax", "complex_np", "yes");

  Overlap join(db, n1, n2);

  unsigned int counter=0;
  for(BinaryMatch m=join.next(); m.found; m=join.next())
  {
    HL_INFO(logger, (boost::format("match\t%1%\t%2%") % db.getNodeName(m.lhs.node) % db.getNodeName(m.rhs.node)).str());
    counter++;
  }

  EXPECT_EQ(47, counter);
}

// mmax:ambiguity="not_ambig" _i_ mmax:complex_np="yes"
TEST_F(SearchTestPcc2, TestQueryInclude) {
  AnnotationNameSearch n1(db, "mmax", "ambiguity", "not_ambig");
  AnnotationNameSearch n2(db, "mmax", "complex_np", "yes");

  Inclusion join(db, n1, n2);

  unsigned int counter=0;
  for(BinaryMatch m=join.next(); m.found; m=join.next())
  {
    HL_INFO(logger, (boost::format("match\t%1%\t%2%") % db.getNodeName(m.lhs.node) % db.getNodeName(m.rhs.node)).str());
    counter++;
  }

  EXPECT_EQ(23, counter);
}



#endif // SEARCHTESTPCC2_H
