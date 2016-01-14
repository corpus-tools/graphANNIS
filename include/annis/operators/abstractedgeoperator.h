#pragma once

#include <annis/db.h>
#include <annis/graphstorage/graphstorage.h>
#include <annis/operators/operator.h>
#include <vector>

namespace annis
{

class AbstractEdgeOperator : public Operator
{
public:
  AbstractEdgeOperator(
      ComponentType componentType,
      const DB& db, std::string ns, std::string name,
      unsigned int minDistance = 1, unsigned int maxDistance = 1);

  AbstractEdgeOperator(
      ComponentType componentType,
      const DB& db, std::string ns, std::string name,
      const Annotation& edgeAnno = Init::initAnnotation());

  virtual std::unique_ptr<AnnoIt> retrieveMatches(const Match& lhs);
  virtual bool filter(const Match& lhs, const Match& rhs);

  virtual bool valid() const {return !gs.empty();}
  
  virtual ~AbstractEdgeOperator();
private:
  ComponentType componentType;
  const DB& db;
  std::string ns;
  std::string name;
  unsigned int minDistance;
  unsigned int maxDistance;
  Annotation anyAnno;
  const Annotation edgeAnno;

  std::vector<const ReadableGraphStorage*> gs;

  void initGraphStorage();
  bool checkEdgeAnnotation(const ReadableGraphStorage *e, nodeid_t source, nodeid_t target);
};

} // end namespace annis
