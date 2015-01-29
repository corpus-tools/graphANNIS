#ifndef ExactAnnoSearch_H
#define ExactAnnoSearch_H

#include "annotationsearch.h"
#include <stx/btree_map>

namespace annis
{

class ExactAnnoSearch : public AnnotationSearch
{
  using ItType = stx::btree_multimap<Annotation, nodeid_t>::const_iterator;

public:

  /**
   * @brief Find annotations by name
   * @param db
   * @param annoName
   */
  ExactAnnoSearch(const DB &db, const std::string& annoNamspace, const std::string& annoName, const std::string& annoValue);

  virtual ~ExactAnnoSearch();

  virtual bool hasNext()
  {
    return it != db.inverseNodeAnnotations.end() && it != itEnd;
  }

  virtual Match next();
  virtual void reset();

  const std::unordered_set<Annotation>& getValidAnnotations()
  {
    if(!validAnnotationInitialized)
    {
      initializeValidAnnotations();
    }
    return validAnnotations;
  }

private:
  const DB& db;

  ItType it;
  ItType itBegin;
  ItType itEnd;

  bool validAnnotationInitialized;
  std::unordered_set<Annotation> validAnnotations;

  bool currentMatchValid;
  Match currentMatch;

  void initializeValidAnnotations();

};


} // end namespace annis
#endif // ExactAnnoSearch_H
