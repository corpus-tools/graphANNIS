#ifndef OVERLAP_H
#define OVERLAP_H

#include <set>
#include <list>

#include <annis/db.h>
#include <annis/iterators.h>
#include <annis/util/helper.h>
#include <annis/operators/operator.h>

namespace annis
{

class Overlap : public Operator
{
public:

  Overlap(const DB &db);

  virtual std::unique_ptr<AnnoIt> retrieveMatches(const Match& lhs);
  virtual bool filter(const Match& lhs, const Match& rhs);


  virtual bool isReflexive() {return false;}

  virtual ~Overlap();
private:
  const DB& db;
  TokenHelper tokHelper;
  Annotation anyNodeAnno;
  const ReadableGraphStorage* gsOrder;
  const ReadableGraphStorage* gsCoverage;
  const ReadableGraphStorage* gsInverseCoverage;
};
} // end namespace annis
#endif // OVERLAP_H
